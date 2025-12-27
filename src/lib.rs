mod ffi_types;
pub mod goals;
pub mod notes;
pub mod session_graph;
pub mod types;

use chrono::{NaiveDate, TimeZone, Utc};
use std::path::Path;

use ffi_types::{AppError, SessionView};

pub use goals::{add_goal, list_goals, search_goals};
pub use notes::{edit_note, get_note};
pub use session_graph::{add_session, get_formatted_session_time_range, list_day_sessions};
pub use types::{Goal, Session, SessionKind};

uniffi::setup_scaffolding!();

#[uniffi::export]
pub fn list_goals_api(archive_path: String) -> Result<Vec<Goal>, AppError> {
    Ok(list_goals(Path::new(&archive_path))?)
}

#[uniffi::export]
pub fn search_goals_api(
    archive_path: String,
    query: String,
    is_reward: Option<bool>,
) -> Result<Vec<Goal>, AppError> {
    Ok(search_goals(Path::new(&archive_path), &query, is_reward)?)
}

#[uniffi::export]
pub fn add_goal_api(
    archive_path: String,
    name: String,
    is_reward: bool,
    commands: Vec<String>,
) -> Result<Goal, AppError> {
    Ok(add_goal(
        Path::new(&archive_path),
        &name,
        is_reward,
        commands,
    )?)
}

#[uniffi::export]
pub fn get_note_api(archive_path: String, goal_id: u64) -> Result<String, AppError> {
    Ok(get_note(Path::new(&archive_path), goal_id)?)
}

#[uniffi::export]
pub fn edit_note_api(
    archive_path: String,
    goal_id: u64,
    content: String,
) -> Result<bool, AppError> {
    edit_note(Path::new(&archive_path), goal_id, &content)?;
    Ok(true)
}

#[uniffi::export]
pub fn add_session_api(
    archive_path: String,
    goal_id: u64,
    goal_name: String,
    start_ts_secs: i64,
    duration_secs: u32,
    is_reward: bool,
) -> Result<SessionView, AppError> {
    let start_at = Utc
        .timestamp_opt(start_ts_secs, 0)
        .single()
        .ok_or_else(|| anyhow::anyhow!("invalid start_ts_secs: {start_ts_secs}"))?;

    Ok(add_session(
        Path::new(&archive_path),
        goal_id,
        &goal_name,
        start_at,
        duration_secs,
        is_reward,
    )
    .map(SessionView::from)?)
}

#[uniffi::export]
pub fn list_day_sessions_api(
    archive_path: String,
    date_iso: String,
) -> Result<Vec<SessionView>, AppError> {
    let date = NaiveDate::parse_from_str(&date_iso, "%Y-%m-%d")
        .map_err(|e| anyhow::anyhow!("date_iso must be YYYY-MM-DD: {e}"))?;

    let sessions = list_day_sessions(Path::new(&archive_path), date)?;
    Ok(sessions.into_iter().map(SessionView::from).collect())
}
