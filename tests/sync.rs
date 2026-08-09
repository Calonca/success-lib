//! Two-device sync integration tests.
//!
//! Each "device" is a real filesystem archive in a tempdir (so the normal
//! public API operates on it); the remote is the in-memory `RemoteStore`
//! fake. All futures are immediately ready, so `pollster` drives them.

use chrono::Utc;
use successlib::goals::get_goal;
use successlib::storage_io::backend_for;
use successlib::sync::engine::sync_archive;
use successlib::sync::remote::MemoryRemote;
use successlib::{
    add_goal, add_session, edit_note, get_note, list_day_sessions, list_goals, GoalStatus,
};
use std::path::Path;
use tempfile::TempDir;

fn device() -> (TempDir, String) {
    let temp = tempfile::tempdir().expect("create temp archive");
    let path = temp.path().to_str().unwrap().to_string();
    (temp, path)
}

fn sync(archive: &str, remote: &MemoryRemote) -> successlib::sync::engine::SyncReport {
    pollster::block_on(sync_archive(&backend_for(Path::new(archive)), remote)).expect("sync")
}

fn all_statuses() -> Option<Vec<GoalStatus>> {
    Some(vec![GoalStatus::TODO, GoalStatus::DOING, GoalStatus::DONE])
}

fn today_iso() -> String {
    Utc::now()
        .with_timezone(&chrono::Local)
        .date_naive()
        .format("%Y-%m-%d")
        .to_string()
}

#[test]
fn fresh_device_pulls_everything() {
    let (_ta, a) = device();
    let (_tb, b) = device();
    let remote = MemoryRemote::new();

    let goal = add_goal(a.clone(), "Learn Rust".into(), false, vec![], None).unwrap();
    edit_note(a.clone(), goal.id, "read the book".into()).unwrap();
    add_session(a.clone(), goal.id, goal.name.clone(), Utc::now().timestamp(), 600, false, None)
        .unwrap();

    let report_a = sync(&a, &remote);
    assert!(report_a.pushed >= 3, "goals, note and day file pushed");

    let report_b = sync(&b, &remote);
    assert!(report_b.pulled >= 3);

    let goals_b = list_goals(b.clone(), all_statuses()).unwrap();
    assert_eq!(goals_b.len(), 1);
    assert_eq!(goals_b[0].name, "Learn Rust");
    assert_eq!(get_note(b.clone(), goal.id).unwrap(), "read the book\n");
    assert_eq!(list_day_sessions(b.clone(), today_iso()).unwrap().len(), 1);
}

#[test]
fn same_day_sessions_from_both_devices_are_merged() {
    let (_ta, a) = device();
    let (_tb, b) = device();
    let remote = MemoryRemote::new();

    let goal = add_goal(a.clone(), "Practice".into(), false, vec![], None).unwrap();
    let base_ts = Utc::now().timestamp();
    add_session(a.clone(), goal.id, goal.name.clone(), base_ts, 600, false, None).unwrap();
    sync(&a, &remote);
    sync(&b, &remote);

    // both devices record a session in the same day file, offline
    add_session(a.clone(), goal.id, goal.name.clone(), base_ts + 1000, 600, false, None).unwrap();
    add_session(b.clone(), goal.id, goal.name.clone(), base_ts + 2000, 600, false, None).unwrap();

    sync(&a, &remote); // A pushes its version
    let report_b = sync(&b, &remote); // B merges
    assert_eq!(report_b.merged, 1);
    sync(&a, &remote); // A pulls the merged file

    let sessions_a = list_day_sessions(a.clone(), today_iso()).unwrap();
    let sessions_b = list_day_sessions(b.clone(), today_iso()).unwrap();
    assert_eq!(sessions_a.len(), 3);
    assert_eq!(sessions_b.len(), 3);
    let starts_a: Vec<i64> = sessions_a.iter().map(|s| s.start_at).collect();
    let starts_b: Vec<i64> = sessions_b.iter().map(|s| s.start_at).collect();
    assert_eq!(starts_a, starts_b);
}

#[test]
fn conflicting_notes_keep_both_texts() {
    let (_ta, a) = device();
    let (_tb, b) = device();
    let remote = MemoryRemote::new();

    let goal = add_goal(a.clone(), "Write".into(), false, vec![], None).unwrap();
    edit_note(a.clone(), goal.id, "original".into()).unwrap();
    sync(&a, &remote);
    sync(&b, &remote);

    edit_note(a.clone(), goal.id, "changed on A".into()).unwrap();
    edit_note(b.clone(), goal.id, "changed on B".into()).unwrap();

    sync(&a, &remote);
    sync(&b, &remote); // B merges with conflict divider
    sync(&a, &remote); // A pulls the merged note

    let note_a = get_note(a.clone(), goal.id).unwrap();
    let note_b = get_note(b.clone(), goal.id).unwrap();
    assert_eq!(note_a, note_b);
    assert!(note_b.contains("changed on A"));
    assert!(note_b.contains("changed on B"));
    assert!(note_b.contains("_Conflicting version from another device:_"));
}

#[test]
fn goals_added_offline_on_both_devices_converge() {
    let (_ta, a) = device();
    let (_tb, b) = device();
    let remote = MemoryRemote::new();

    sync(&a, &remote);
    sync(&b, &remote);

    let goal_a = add_goal(a.clone(), "Goal from A".into(), false, vec![], None).unwrap();
    let goal_b = add_goal(b.clone(), "Goal from B".into(), false, vec![], None).unwrap();

    sync(&a, &remote);
    sync(&b, &remote);
    sync(&a, &remote);

    for archive in [&a, &b] {
        let goals = list_goals(archive.clone(), all_statuses()).unwrap();
        assert_eq!(goals.len(), 2, "both goals visible on {archive}");
        assert!(get_goal(Path::new(archive), goal_a.id).is_ok());
        assert!(get_goal(Path::new(archive), goal_b.id).is_ok());
    }
}

#[test]
fn status_change_propagates_without_conflict() {
    let (_ta, a) = device();
    let (_tb, b) = device();
    let remote = MemoryRemote::new();

    let goal = add_goal(a.clone(), "Meditate".into(), false, vec![], None).unwrap();
    sync(&a, &remote);
    sync(&b, &remote);

    successlib::set_goal_status(b.clone(), goal.id, GoalStatus::DONE).unwrap();
    sync(&b, &remote);
    sync(&a, &remote);

    let goal_on_a = get_goal(Path::new(&a), goal.id).unwrap();
    assert_eq!(goal_on_a.status, GoalStatus::DONE);
}

#[test]
fn sync_state_is_never_uploaded() {
    let (_ta, a) = device();
    let remote = MemoryRemote::new();

    let goal = add_goal(a.clone(), "Private".into(), false, vec![], None).unwrap();
    edit_note(a.clone(), goal.id, "note".into()).unwrap();
    sync(&a, &remote);
    sync(&a, &remote); // second run: state files exist locally now

    assert!(
        remote.paths().iter().all(|p| !p.starts_with(".sync/")),
        "remote must not contain local sync state, got {:?}",
        remote.paths()
    );
}

#[test]
fn repeated_sync_is_idempotent() {
    let (_ta, a) = device();
    let remote = MemoryRemote::new();

    add_goal(a.clone(), "Stable".into(), false, vec![], None).unwrap();
    sync(&a, &remote);
    let report = sync(&a, &remote);
    assert_eq!(report.pushed, 0);
    assert_eq!(report.pulled, 0);
    assert_eq!(report.merged, 0);
}
