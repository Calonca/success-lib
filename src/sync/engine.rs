//! The offline-first sync algorithm.
//!
//! For each file the engine compares the local content against the base
//! snapshot from the last sync (local change detection) and the remote
//! revision against the last-synced revision (remote change detection),
//! then pushes, pulls, or three-way merges. `goals.yaml` is handled first
//! so that goal-id reassignments (legacy sequential-id collisions) can be
//! applied to notes and session files before those are merged.

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;

use crate::backend::StorageBackend;
use crate::ffi_types::AppError;
use crate::session_graph::{parse_mermaid, to_mermaid};
use crate::sync::merge::{merge_goals, merge_notes, merge_sessions};
use crate::sync::remote::{RemoteStore, SyncError};
use crate::sync::state::{self, SyncState};
use crate::types::{Goal, Session};

/// Outcome counts of a [`sync_archive`] run.
#[cfg_attr(all(not(target_arch = "wasm32"), feature = "uniffi"), derive(uniffi::Record))]
#[derive(Debug, Default, Clone)]
pub struct SyncReport {
    /// Files whose local version was uploaded.
    pub pushed: u32,
    /// Files whose remote version was downloaded.
    pub pulled: u32,
    /// Files that changed on both sides and were three-way merged
    /// (the merged result is also uploaded, but counted only here).
    pub merged: u32,
}

const GOALS_PATH: &str = "goals.yaml";
const MAX_PUT_RETRIES: u32 = 3;

/// Bidirectionally sync the archive behind `backend` with `remote`.
pub async fn sync_archive<B: StorageBackend, R: RemoteStore>(
    backend: &B,
    remote: &R,
) -> Result<SyncReport, AppError> {
    backend.ensure_structure()?;
    let mut sync_state = state::load_state(backend)?;
    let mut report = SyncReport::default();

    let remote_revisions: BTreeMap<String, i64> = remote
        .list()
        .await?
        .into_iter()
        .filter(|e| is_syncable(&e.path))
        .map(|e| (e.path, e.revision))
        .collect();

    // goals.yaml first: id reassignments must land before notes/graphs merge.
    sync_goals(backend, remote, &mut sync_state, &remote_revisions, &mut report).await?;
    state::save_state(backend, &sync_state)?;

    let mut paths: BTreeSet<String> = backend
        .list("")?
        .into_iter()
        .filter(|p| is_syncable(p))
        .collect();
    paths.extend(remote_revisions.keys().cloned());
    paths.remove(GOALS_PATH);

    for path in paths {
        sync_file(
            backend,
            remote,
            &mut sync_state,
            &remote_revisions,
            &path,
            &mut report,
        )
        .await?;
        state::save_state(backend, &sync_state)?;
    }

    Ok(report)
}

/// What happened to one file during sync (relative to the last-synced state).
struct FileStatus {
    local: Option<String>,
    base: Option<String>,
    state_revision: Option<i64>,
    remote_revision: Option<i64>,
}

impl FileStatus {
    fn load<B: StorageBackend>(
        backend: &B,
        sync_state: &SyncState,
        remote_revisions: &BTreeMap<String, i64>,
        path: &str,
    ) -> Result<Self, AppError> {
        Ok(Self {
            local: backend.read(path)?,
            base: state::read_base(backend, path)?,
            state_revision: sync_state.revisions.get(path).copied(),
            remote_revision: remote_revisions.get(path).copied(),
        })
    }

    fn local_changed(&self) -> bool {
        self.local != self.base
    }

    fn remote_changed(&self) -> bool {
        self.remote_revision != self.state_revision
    }
}

/// Record a completed transfer: base snapshot + revision bookkeeping.
fn record_synced<B: StorageBackend>(
    backend: &B,
    sync_state: &mut SyncState,
    path: &str,
    content: &str,
    revision: i64,
) -> Result<(), AppError> {
    state::write_base(backend, path, content)?;
    sync_state.revisions.insert(path.to_string(), revision);
    Ok(())
}

async fn sync_goals<B: StorageBackend, R: RemoteStore>(
    backend: &B,
    remote: &R,
    sync_state: &mut SyncState,
    remote_revisions: &BTreeMap<String, i64>,
    report: &mut SyncReport,
) -> Result<(), AppError> {
    let status = FileStatus::load(backend, sync_state, remote_revisions, GOALS_PATH)?;
    // A never-synced goals.yaml holding no goals is what ensure_structure
    // scaffolds on a fresh device — not a local edit.
    let local_changed = status.local_changed()
        && !(status.base.is_none()
            && parse_goals(status.local.as_deref().unwrap_or("[]"))
                .map(|goals| goals.is_empty())
                .unwrap_or(false));
    match (local_changed, status.remote_changed()) {
        (false, false) => Ok(()),
        (false, true) => pull(backend, remote, sync_state, GOALS_PATH, report).await,
        (true, false) => {
            let Some(content) = &status.local else {
                return Ok(());
            };
            match remote.put(GOALS_PATH, content, status.state_revision).await {
                Ok(revision) => {
                    record_synced(backend, sync_state, GOALS_PATH, content, revision)?;
                    report.pushed += 1;
                    Ok(())
                }
                // Someone synced concurrently: merge instead of failing.
                Err(SyncError::Conflict) => {
                    merge_and_push_goals(backend, remote, sync_state, &status, report).await
                }
                Err(e) => Err(e.into()),
            }
        }
        (true, true) => merge_and_push_goals(backend, remote, sync_state, &status, report).await,
    }
}

/// Merge a both-sides-changed `goals.yaml` and upload it, retrying on
/// concurrent-writer conflicts.
async fn merge_and_push_goals<B: StorageBackend, R: RemoteStore>(
    backend: &B,
    remote: &R,
    sync_state: &mut SyncState,
    status: &FileStatus,
    report: &mut SyncReport,
) -> Result<(), AppError> {
    let base_goals = parse_goals(status.base.as_deref().unwrap_or("[]"))?;
    let mut local_goals = parse_goals(status.local.as_deref().unwrap_or("[]"))?;
    let mut attempts = 0;
    let mut remote_doc = remote.get(GOALS_PATH).await?;
    loop {
        let (remote_content, remote_revision) = match &remote_doc {
            Some(doc) => (doc.content.clone(), Some(doc.revision)),
            None => ("[]".to_string(), None),
        };
        let remote_goals = parse_goals(&remote_content)?;
        let result = merge_goals(&base_goals, &local_goals, &remote_goals);
        if !result.reassigned.is_empty() {
            apply_reassignments(backend, &result.reassigned)?;
            // Keep the in-memory local view in step, so a put-conflict retry
            // re-merges with the already-assigned ids instead of detecting
            // the same collision again and minting fresh ones (which would
            // orphan the just-renamed notes and rewritten sessions).
            for goal in &mut local_goals {
                if let Some((_, new_id)) =
                    result.reassigned.iter().find(|(old, _)| *old == goal.id)
                {
                    goal.id = *new_id;
                }
            }
        }
        let merged_yaml = serde_yaml::to_string(&result.merged)?;
        backend.write(GOALS_PATH, &merged_yaml)?;

        match remote.put(GOALS_PATH, &merged_yaml, remote_revision).await {
            Ok(revision) => {
                record_synced(backend, sync_state, GOALS_PATH, &merged_yaml, revision)?;
                report.merged += 1;
                return Ok(());
            }
            Err(SyncError::Conflict) if attempts < MAX_PUT_RETRIES => {
                attempts += 1;
                remote_doc = remote.get(GOALS_PATH).await?;
            }
            Err(e) => return Err(e.into()),
        }
    }
}

async fn sync_file<B: StorageBackend, R: RemoteStore>(
    backend: &B,
    remote: &R,
    sync_state: &mut SyncState,
    remote_revisions: &BTreeMap<String, i64>,
    path: &str,
    report: &mut SyncReport,
) -> Result<(), AppError> {
    let status = FileStatus::load(backend, sync_state, remote_revisions, path)?;
    match (status.local_changed(), status.remote_changed()) {
        (false, false) => Ok(()),
        (true, false) => {
            let Some(content) = &status.local else {
                return Ok(());
            };
            match remote.put(path, content, status.state_revision).await {
                Ok(revision) => {
                    record_synced(backend, sync_state, path, content, revision)?;
                    report.pushed += 1;
                    Ok(())
                }
                // Someone synced concurrently: merge instead of failing.
                Err(SyncError::Conflict) => {
                    merge_and_push(backend, remote, sync_state, path, &status, report).await
                }
                Err(e) => Err(e.into()),
            }
        }
        (false, true) => {
            if status.remote_revision.is_none() {
                // Remote entry disappeared; forget it so a future local
                // change is pushed as a new document.
                sync_state.revisions.remove(path);
                return Ok(());
            }
            pull(backend, remote, sync_state, path, report).await
        }
        (true, true) => {
            merge_and_push(backend, remote, sync_state, path, &status, report).await
        }
    }
}

/// Merge a both-sides-changed file and upload it, retrying on
/// concurrent-writer conflicts.
async fn merge_and_push<B: StorageBackend, R: RemoteStore>(
    backend: &B,
    remote: &R,
    sync_state: &mut SyncState,
    path: &str,
    status: &FileStatus,
    report: &mut SyncReport,
) -> Result<(), AppError> {
    let local_content = status.local.clone().unwrap_or_default();
    let mut attempts = 0;
    let mut remote_doc = remote.get(path).await?;
    loop {
        let (merged, remote_revision) = match &remote_doc {
            Some(doc) => (
                merge_file(path, status.base.as_deref(), &local_content, &doc.content),
                Some(doc.revision),
            ),
            // Remote vanished: push the local version as new.
            None => (local_content.clone(), None),
        };
        backend.write(path, &merged)?;

        match remote.put(path, &merged, remote_revision).await {
            Ok(revision) => {
                record_synced(backend, sync_state, path, &merged, revision)?;
                report.merged += 1;
                return Ok(());
            }
            Err(SyncError::Conflict) if attempts < MAX_PUT_RETRIES => {
                attempts += 1;
                remote_doc = remote.get(path).await?;
            }
            Err(e) => return Err(e.into()),
        }
    }
}

/// Pull a remotely-changed file whose local side is unchanged.
async fn pull<B: StorageBackend, R: RemoteStore>(
    backend: &B,
    remote: &R,
    sync_state: &mut SyncState,
    path: &str,
    report: &mut SyncReport,
) -> Result<(), AppError> {
    let Some(doc) = remote.get(path).await? else {
        sync_state.revisions.remove(path);
        return Ok(());
    };
    backend.write(path, &doc.content)?;
    record_synced(backend, sync_state, path, &doc.content, doc.revision)?;
    report.pulled += 1;
    Ok(())
}

/// Merge a both-sides-changed file according to its type.
///
/// A remote day file that fails to parse is treated as empty, which keeps
/// the local sessions and re-uploads them over the malformed content.
fn merge_file(path: &str, base: Option<&str>, local: &str, remote: &str) -> String {
    if let Some(date) = day_file_date(path) {
        let local_sessions = parse_mermaid(local, date).unwrap_or_default();
        let remote_sessions = parse_mermaid(remote, date).unwrap_or_default();
        let merged = merge_sessions(&local_sessions, &remote_sessions);
        to_mermaid(&merged)
    } else {
        merge_notes(base.unwrap_or_default(), local, remote)
    }
}

/// `graphs/YYYY-MM-DD.mmd` → the date, otherwise `None`.
fn day_file_date(path: &str) -> Option<NaiveDate> {
    let name = path.strip_prefix("graphs/")?.strip_suffix(".mmd")?;
    NaiveDate::parse_from_str(name, "%Y-%m-%d").ok()
}

/// Whether a path is part of the archive's synced data.
///
/// Anything else — `.sync/` bookkeeping, stray files dropped into the
/// archive directory (`.DS_Store`, editor backups, possibly non-UTF-8), or
/// foreign localStorage keys that happen to share the archive prefix — is
/// neither read, uploaded, nor pulled.
fn is_syncable(path: &str) -> bool {
    if path == GOALS_PATH {
        return true;
    }
    if let Some(rest) = path.strip_prefix("graphs/") {
        return !rest.contains('/') && day_file_date(path).is_some();
    }
    if let Some(rest) = path.strip_prefix("notes/") {
        return !rest.contains('/') && rest.ends_with(".md");
    }
    false
}

fn parse_goals(content: &str) -> Result<Vec<Goal>, AppError> {
    Ok(serde_yaml::from_str(content)?)
}

/// Rewrite local notes and session files after goal-id reassignments.
fn apply_reassignments<B: StorageBackend>(
    backend: &B,
    reassigned: &[(u64, u64)],
) -> Result<(), AppError> {
    if reassigned.is_empty() {
        return Ok(());
    }
    let id_map: BTreeMap<u64, u64> = reassigned.iter().copied().collect();

    for (old_id, new_id) in reassigned {
        let old_path = format!("notes/goal_{old_id}.md");
        if let Some(content) = backend.read(&old_path)? {
            backend.write(&format!("notes/goal_{new_id}.md"), &content)?;
            backend.delete(&old_path)?;
        }
    }

    for path in backend.list("graphs/")? {
        let Some(date) = day_file_date(&path) else {
            continue;
        };
        let Some(content) = backend.read(&path)? else {
            continue;
        };
        let mut sessions: Vec<Session> = parse_mermaid(&content, date).unwrap_or_default();
        let mut changed = false;
        for session in &mut sessions {
            if let Some(new_id) = id_map.get(&session.goal_id) {
                session.goal_id = *new_id;
                changed = true;
            }
        }
        if changed {
            sessions.sort_by_key(|s| s.start_at);
            backend.write(&path, &to_mermaid(&sessions))?;
        }
    }
    Ok(())
}
