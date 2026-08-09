//! Local sync bookkeeping, stored under the `.sync/` prefix of the archive.
//!
//! - `.sync/state.json`: per path, the remote revision seen at last sync.
//! - `.sync/base/<path>`: snapshot of each file's content at last sync,
//!   used as the base for three-way merges. A missing snapshot means the
//!   file has never been synced.
//!
//! The `.sync/` prefix is local-only and must never be pushed to a remote.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::backend::StorageBackend;
use crate::ffi_types::AppError;

pub const SYNC_PREFIX: &str = ".sync/";
const STATE_PATH: &str = ".sync/state.json";
const BASE_PREFIX: &str = ".sync/base/";

/// Per-file remote revisions recorded at the last successful sync.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SyncState {
    #[serde(default)]
    pub revisions: BTreeMap<String, i64>,
}

pub fn load_state(backend: &dyn StorageBackend) -> Result<SyncState, AppError> {
    let Some(data) = backend.read(STATE_PATH)? else {
        return Ok(SyncState::default());
    };
    serde_json::from_str(&data).map_err(|e| AppError::Parse {
        detail: format!("corrupt {STATE_PATH}: {e}"),
    })
}

pub fn save_state(backend: &dyn StorageBackend, state: &SyncState) -> Result<(), AppError> {
    let data = serde_json::to_string_pretty(state).map_err(|e| AppError::Parse {
        detail: e.to_string(),
    })?;
    backend.write(STATE_PATH, &data)?;
    Ok(())
}

/// Content of `path` as of the last successful sync, if any.
pub fn read_base(
    backend: &dyn StorageBackend,
    path: &str,
) -> Result<Option<String>, AppError> {
    Ok(backend.read(&format!("{BASE_PREFIX}{path}"))?)
}

pub fn write_base(
    backend: &dyn StorageBackend,
    path: &str,
    content: &str,
) -> Result<(), AppError> {
    backend.write(&format!("{BASE_PREFIX}{path}"), content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::MemoryBackend;

    #[test]
    fn missing_state_is_empty() {
        let b = MemoryBackend::new();
        let state = load_state(&b).unwrap();
        assert!(state.revisions.is_empty());
    }

    #[test]
    fn state_roundtrip() {
        let b = MemoryBackend::new();
        let mut state = SyncState::default();
        state.revisions.insert("goals.yaml".into(), 3);
        save_state(&b, &state).unwrap();

        let loaded = load_state(&b).unwrap();
        assert_eq!(loaded.revisions.get("goals.yaml"), Some(&3));
    }

    #[test]
    fn base_snapshot_roundtrip() {
        let b = MemoryBackend::new();
        assert_eq!(read_base(&b, "notes/goal_1.md").unwrap(), None);
        write_base(&b, "notes/goal_1.md", "hello\n").unwrap();
        assert_eq!(
            read_base(&b, "notes/goal_1.md").unwrap(),
            Some("hello\n".to_string())
        );
        // snapshots live under the local-only .sync/ prefix
        assert_eq!(
            b.read(".sync/base/notes/goal_1.md").unwrap(),
            Some("hello\n".to_string())
        );
    }
}
