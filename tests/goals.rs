use chrono::Utc;
use successlib::{
    add_goal, add_session, list_goals, list_trash, search_goals, set_goal_status, set_goal_trashed,
    GoalStatus,
};
use tempfile::TempDir;

fn temp_archive() -> TempDir {
    tempfile::tempdir().expect("create temp archive")
}

#[test]
fn list_goals_filters_by_status() {
    let temp = temp_archive();
    let archive = temp.path().to_str().unwrap().to_string();

    let g1 = add_goal(archive.clone(), "Goal 1".into(), false, vec![], None).unwrap();
    let g2 = add_goal(archive.clone(), "Goal 2".into(), false, vec![], None).unwrap();
    set_goal_status(archive.clone(), g1.id, GoalStatus::DONE).unwrap();

    let default_visible = list_goals(archive.clone(), None).unwrap();
    assert_eq!(default_visible.len(), 1);
    assert_eq!(default_visible[0].id, g2.id);

    let done_only = list_goals(archive.clone(), Some(vec![GoalStatus::DONE])).unwrap();
    assert_eq!(done_only.len(), 1);
    assert_eq!(done_only[0].id, g1.id);

    let union = list_goals(archive.clone(), Some(vec![GoalStatus::DONE, GoalStatus::TODO])).unwrap();
    let mut ids: Vec<u64> = union.into_iter().map(|g| g.id).collect();
    ids.sort_unstable();
    assert_eq!(ids, vec![g1.id, g2.id]);
}

#[test]
fn search_goals_respects_status_filter() {
    let temp = temp_archive();
    let archive = temp.path().to_str().unwrap().to_string();

    let g1 = add_goal(archive.clone(), "Archive Docs".into(), false, vec![], None).unwrap();
    let g2 = add_goal(archive.clone(), "Build Prototype".into(), false, vec![], None).unwrap();
    set_goal_status(archive.clone(), g1.id, GoalStatus::DONE).unwrap();
    set_goal_status(archive.clone(), g2.id, GoalStatus::DOING).unwrap();

    let default_results = search_goals(archive.clone(), "".into(), None, None, Some(false)).unwrap();
    assert_eq!(default_results.len(), 1);
    assert_eq!(default_results[0].id, g2.id);

    let done_results = search_goals(
        archive.clone(),
        "arch".into(),
        None,
        Some(vec![GoalStatus::DONE]),
        Some(false),
    )
    .unwrap();
    assert_eq!(done_results.len(), 1);
    assert_eq!(done_results[0].id, g1.id);
}

#[test]
fn adding_session_moves_goal_to_doing() {
    let temp = temp_archive();
    let archive = temp.path().to_str().unwrap().to_string();

    let goal = add_goal(archive.clone(), "Practice guitar".into(), false, vec![], None).unwrap();
    add_session(
        archive.clone(),
        goal.id,
        goal.name.clone(),
        Utc::now().timestamp(),
        600,
        false,
        None,
    )
    .unwrap();

    let goals = list_goals(archive.clone(), Some(vec![GoalStatus::DOING])).unwrap();
    assert_eq!(goals.len(), 1);
    assert_eq!(goals[0].id, goal.id);
    assert_eq!(goals[0].status, GoalStatus::DOING);
}

#[test]
fn trashed_goals_are_hidden_and_listtrash_works() {
    let temp = temp_archive();
    let archive = temp.path().to_str().unwrap().to_string();

    let trashed = add_goal(archive.clone(), "Old goal".into(), false, vec![], None).unwrap();
    let active = add_goal(archive.clone(), "New goal".into(), false, vec![], None).unwrap();
    set_goal_trashed(archive.clone(), trashed.id, true).unwrap();

    let visible = list_goals(archive.clone(), None).unwrap();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].id, active.id);

    let trashed_items = list_trash(archive.clone()).unwrap();
    assert_eq!(trashed_items.len(), 1);
    assert_eq!(trashed_items[0].id, trashed.id);

    let search_trashed = search_goals(archive.clone(), "Old".into(), None, None, Some(false)).unwrap();
    assert!(search_trashed.is_empty());
}

#[test]
fn reward_session_parsing_preserves_goal_id() {
    let temp = temp_archive();
    let archive = temp.path().to_str().unwrap().to_string();

    let reward = add_goal(archive.clone(), "Ice Cream".into(), true, vec![], None).unwrap();
    let now = Utc::now();
    let date_iso = now.with_timezone(&chrono::Local).date_naive().format("%Y-%m-%d").to_string();
    
    add_session(
        archive.clone(),
        reward.id,
        reward.name.clone(),
        now.timestamp(),
        300,
        true,
        None,
    )
    .unwrap();

    let sessions = successlib::list_day_sessions(archive.clone(), date_iso).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].goal_id, reward.id);
}
