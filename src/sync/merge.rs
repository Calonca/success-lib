//! Pure three-way merge functions for the archive's file types.
//!
//! Each function merges a local and a remote version of the same logical
//! file, using the last-synced version as the base where three-way context
//! is needed. They never perform I/O.

use std::collections::BTreeMap;

use crate::goals::new_goal_id;
use crate::types::{Goal, Session, SessionKind};

/// Merge two versions of a day's session list.
///
/// Sessions are identified by `(goal_id, start_at, end_at, kind)`; the union
/// of both sides is kept (local wins on identical keys, e.g. differing
/// quantities), sorted by start time, and ids are renumbered `sess_N` /
/// `rew_N` the same way `session_graph` numbers them on save.
pub fn merge_sessions(local: &[Session], remote: &[Session]) -> Vec<Session> {
    let mut by_key: BTreeMap<(u64, i64, i64, bool), Session> = BTreeMap::new();
    for session in remote.iter().chain(local.iter()) {
        let key = (
            session.goal_id,
            session.start_at,
            session.end_at,
            session.kind == SessionKind::Reward,
        );
        by_key.insert(key, session.clone());
    }

    let mut merged: Vec<Session> = by_key.into_values().collect();
    merged.sort_by_key(|s| s.start_at);

    let mut goal_counter = 0u32;
    let mut reward_counter = 0u32;
    for session in &mut merged {
        session.id = match session.kind {
            SessionKind::Goal => {
                goal_counter += 1;
                format!("sess_{goal_counter}")
            }
            SessionKind::Reward => {
                reward_counter += 1;
                format!("rew_{reward_counter}")
            }
        };
    }
    merged
}

/// Result of merging `goals.yaml`.
pub struct GoalsMergeResult {
    pub merged: Vec<Goal>,
    /// `(old_id, new_id)` pairs for local goals that had to be re-identified
    /// because an unrelated remote goal used the same id (legacy sequential
    /// ids). The caller must rewrite local notes and sessions accordingly.
    pub reassigned: Vec<(u64, u64)>,
}

/// Three-way merge of the goal list, per goal id.
///
/// - present on one side only: kept.
/// - present on both, equal: kept.
/// - present on both, one side equals base: the changed side wins.
/// - present on both, both changed: local wins (deterministic).
/// - present on both but absent from base with different names: these are two
///   distinct goals that collided on a legacy sequential id — the remote goal
///   keeps the id and the local goal is reassigned a fresh random id.
pub fn merge_goals(base: &[Goal], local: &[Goal], remote: &[Goal]) -> GoalsMergeResult {
    let base_by_id: BTreeMap<u64, &Goal> = base.iter().map(|g| (g.id, g)).collect();
    let remote_by_id: BTreeMap<u64, &Goal> = remote.iter().map(|g| (g.id, g)).collect();
    let local_ids: std::collections::BTreeSet<u64> = local.iter().map(|g| g.id).collect();

    let mut merged: Vec<Goal> = Vec::new();
    let mut collided: Vec<Goal> = Vec::new();
    let mut reassigned = Vec::new();

    for local_goal in local {
        match remote_by_id.get(&local_goal.id) {
            None => merged.push(local_goal.clone()),
            Some(remote_goal) => {
                let base_goal = base_by_id.get(&local_goal.id);
                if base_goal.is_none()
                    && local_goal.name != remote_goal.name
                {
                    // Two distinct goals minted the same legacy id offline.
                    merged.push((*remote_goal).clone());
                    collided.push(local_goal.clone());
                } else if local_goal == *remote_goal {
                    merged.push(local_goal.clone());
                } else if base_goal.is_some_and(|b| *b == local_goal) {
                    merged.push((*remote_goal).clone());
                } else {
                    // local changed (remote unchanged, or both changed).
                    merged.push(local_goal.clone());
                }
            }
        }
    }

    for remote_goal in remote {
        if !local_ids.contains(&remote_goal.id) {
            merged.push(remote_goal.clone());
        }
    }

    for mut local_goal in collided {
        let old_id = local_goal.id;
        let new_id = new_goal_id(&merged);
        local_goal.id = new_id;
        merged.push(local_goal);
        reassigned.push((old_id, new_id));
    }

    GoalsMergeResult { merged, reassigned }
}

/// Three-way merge of a note's Markdown content.
///
/// If only one side changed since `base`, that side wins. If both changed,
/// both versions are kept, separated by a conflict divider, so no text is
/// ever silently dropped.
pub fn merge_notes(base: &str, local: &str, remote: &str) -> String {
    if local == remote || remote == base {
        return local.to_string();
    }
    if local == base {
        return remote.to_string();
    }
    format!(
        "{}\n\n---\n_Conflicting version from another device:_\n---\n\n{}",
        local.trim_end_matches('\n'),
        remote.trim_end_matches('\n')
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::GoalStatus;

    fn session(goal_id: u64, start_at: i64, kind: SessionKind) -> Session {
        Session {
            id: "x".into(),
            name: format!("goal {goal_id}"),
            goal_id,
            kind,
            quantity: None,
            start_at,
            end_at: start_at + 600,
        }
    }

    fn goal(id: u64, name: &str, status: GoalStatus) -> Goal {
        Goal {
            id,
            name: name.into(),
            is_reward: false,
            commands: vec![],
            status,
            trashed: false,
            quantity_name: None,
        }
    }

    #[test]
    fn merge_sessions_unions_and_sorts() {
        let local = vec![session(1, 100, SessionKind::Goal)];
        let remote = vec![session(2, 50, SessionKind::Goal)];
        let merged = merge_sessions(&local, &remote);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].goal_id, 2);
        assert_eq!(merged[1].goal_id, 1);
        assert_eq!(merged[0].id, "sess_1");
        assert_eq!(merged[1].id, "sess_2");
    }

    #[test]
    fn merge_sessions_dedupes_identical_sessions() {
        let a = vec![session(1, 100, SessionKind::Goal)];
        let merged = merge_sessions(&a, &a);
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn merge_sessions_local_wins_on_key_clash() {
        let mut local = session(1, 100, SessionKind::Goal);
        local.quantity = Some(5);
        let mut remote = session(1, 100, SessionKind::Goal);
        remote.quantity = Some(9);
        let merged = merge_sessions(&[local], &[remote]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].quantity, Some(5));
    }

    #[test]
    fn merge_sessions_numbers_rewards_separately() {
        let local = vec![
            session(1, 100, SessionKind::Goal),
            session(2, 200, SessionKind::Reward),
        ];
        let remote = vec![session(3, 150, SessionKind::Goal)];
        let merged = merge_sessions(&local, &remote);
        let ids: Vec<&str> = merged.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["sess_1", "sess_2", "rew_1"]);
    }

    #[test]
    fn merge_goals_keeps_one_side_additions() {
        let base = vec![];
        let local = vec![goal(1, "local goal", GoalStatus::TODO)];
        let remote = vec![goal(2, "remote goal", GoalStatus::TODO)];
        let result = merge_goals(&base, &local, &remote);
        assert_eq!(result.merged.len(), 2);
        assert!(result.reassigned.is_empty());
    }

    #[test]
    fn merge_goals_changed_side_wins() {
        let base = vec![goal(1, "study", GoalStatus::TODO)];
        let local = vec![goal(1, "study", GoalStatus::TODO)];
        let remote = vec![goal(1, "study", GoalStatus::DOING)];
        let result = merge_goals(&base, &local, &remote);
        assert_eq!(result.merged.len(), 1);
        assert_eq!(result.merged[0].status, GoalStatus::DOING);

        // symmetric: local changed, remote untouched
        let result = merge_goals(&base, &remote, &local);
        assert_eq!(result.merged[0].status, GoalStatus::DOING);
    }

    #[test]
    fn merge_goals_both_changed_local_wins() {
        let base = vec![goal(1, "study", GoalStatus::TODO)];
        let local = vec![goal(1, "study", GoalStatus::DONE)];
        let remote = vec![goal(1, "study", GoalStatus::DOING)];
        let result = merge_goals(&base, &local, &remote);
        assert_eq!(result.merged.len(), 1);
        assert_eq!(result.merged[0].status, GoalStatus::DONE);
    }

    #[test]
    fn merge_goals_reassigns_legacy_id_collision() {
        let base = vec![];
        let local = vec![goal(1, "practice piano", GoalStatus::TODO)];
        let remote = vec![goal(1, "write novel", GoalStatus::TODO)];
        let result = merge_goals(&base, &local, &remote);

        assert_eq!(result.merged.len(), 2);
        assert_eq!(result.reassigned.len(), 1);
        let (old_id, new_id) = result.reassigned[0];
        assert_eq!(old_id, 1);
        assert_ne!(new_id, 1);

        let remote_kept = result.merged.iter().find(|g| g.id == 1).unwrap();
        assert_eq!(remote_kept.name, "write novel");
        let local_moved = result.merged.iter().find(|g| g.id == new_id).unwrap();
        assert_eq!(local_moved.name, "practice piano");
    }

    #[test]
    fn merge_notes_one_side_changed() {
        assert_eq!(merge_notes("base\n", "base\n", "remote\n"), "remote\n");
        assert_eq!(merge_notes("base\n", "local\n", "base\n"), "local\n");
        assert_eq!(merge_notes("base\n", "same\n", "same\n"), "same\n");
    }

    #[test]
    fn merge_notes_both_changed_keeps_both() {
        let merged = merge_notes("base\n", "local text\n", "remote text\n");
        assert!(merged.contains("local text"));
        assert!(merged.contains("remote text"));
        assert!(merged.contains("_Conflicting version from another device:_"));
    }
}
