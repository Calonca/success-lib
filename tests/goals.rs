use anyhow::Result;
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
fn list_goals_filters_by_status() -> Result<()> {
    let temp = temp_archive();
    let archive = temp.path();

    let g1 = add_goal(archive, "Goal 1", false, vec![], None)?;
    let g2 = add_goal(archive, "Goal 2", false, vec![], None)?;
    set_goal_status(archive, g1.id, GoalStatus::DONE)?;

    let default_visible = list_goals(archive, None)?;
    assert_eq!(default_visible.len(), 1);
    assert_eq!(default_visible[0].id, g2.id);

    let done_only = list_goals(archive, Some(&[GoalStatus::DONE]))?;
    assert_eq!(done_only.len(), 1);
    assert_eq!(done_only[0].id, g1.id);

    let union = list_goals(archive, Some(&[GoalStatus::DONE, GoalStatus::TODO]))?;
    let mut ids: Vec<u64> = union.into_iter().map(|g| g.id).collect();
    ids.sort_unstable();
    assert_eq!(ids, vec![g1.id, g2.id]);

    Ok(())
}

#[test]
fn search_goals_respects_status_filter() -> Result<()> {
    let temp = temp_archive();
    let archive = temp.path();

    let g1 = add_goal(archive, "Archive Docs", false, vec![], None)?;
    let g2 = add_goal(archive, "Build Prototype", false, vec![], None)?;
    set_goal_status(archive, g1.id, GoalStatus::DONE)?;
    set_goal_status(archive, g2.id, GoalStatus::DOING)?;

    let default_results = search_goals(archive, "", None, None, false)?;
    assert_eq!(default_results.len(), 1);
    assert_eq!(default_results[0].id, g2.id);

    let done_results = search_goals(archive, "arch", None, Some(&[GoalStatus::DONE]), false)?;
    assert_eq!(done_results.len(), 1);
    assert_eq!(done_results[0].id, g1.id);

    Ok(())
}

#[test]
fn adding_session_moves_goal_to_doing() -> Result<()> {
    let temp = temp_archive();
    let archive = temp.path();

    let goal = add_goal(archive, "Practice guitar", false, vec![], None)?;
    add_session(archive, goal.id, &goal.name, Utc::now(), 600, false, None)?;

    let goals = list_goals(archive, Some(&[GoalStatus::DOING]))?;
    assert_eq!(goals.len(), 1);
    assert_eq!(goals[0].id, goal.id);
    assert_eq!(goals[0].status, GoalStatus::DOING);

    Ok(())
}

#[test]
fn trashed_goals_are_hidden_and_listtrash_works() -> Result<()> {
    let temp = temp_archive();
    let archive = temp.path();

    let trashed = add_goal(archive, "Old goal", false, vec![], None)?;
    let active = add_goal(archive, "New goal", false, vec![], None)?;
    set_goal_trashed(archive, trashed.id, true)?;

    let visible = list_goals(archive, None)?;
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].id, active.id);

    let trashed_items = list_trash(archive)?;
    assert_eq!(trashed_items.len(), 1);
    assert_eq!(trashed_items[0].id, trashed.id);

    let search_trashed = search_goals(archive, "Old", None, None, false)?;
    assert!(search_trashed.is_empty());

    Ok(())
}

#[test]
fn reward_session_parsing_preserves_goal_id() -> Result<()> {
    let temp = temp_archive();
    let archive = temp.path();

    let reward = add_goal(archive, "Ice Cream", true, vec![], None)?;
    let now = Utc::now();
    add_session(archive, reward.id, &reward.name, now, 300, true, None)?;

    let sessions =
        successlib::list_day_sessions(archive, now.with_timezone(&chrono::Local).date_naive())?;
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].goal_id, reward.id);
    assert!(matches!(sessions[0].kind, successlib::SessionKind::Reward));

    Ok(())
}
