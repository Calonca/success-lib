//! FFI bindings for the Success note-taking app.
//!
//! This crate exposes a small, focused set of functions for listing and
//! manipulating goals, notes, and sessions. The functions are exported via
//! `uniffi` for use by language bindings.
mod ffi_types;

// Hide internal module pages from the crate-level docs; the re-exported
// items are still visible at the crate root and will appear in the docs.
#[doc(hidden)]
pub mod goals;
#[doc(hidden)]
pub mod notes;
#[doc(hidden)]
pub mod session_graph;
#[doc(hidden)]
mod storage_io;
#[doc(hidden)]
pub mod types;

use chrono::{NaiveDate, TimeZone, Utc};
use std::path::Path;

use ffi_types::AppError;
pub use ffi_types::SessionView;

pub use ffi_types::AppError as Error;
pub use types::{Goal, GoalStatus, Session, SessionKind};

#[cfg(not(target_arch = "wasm32"))]
uniffi::setup_scaffolding!();

/// List goals stored in the archive at `archive_path`.
///
/// - `archive_path`: path to the archive directory.
/// - `statuses`: optional filter to restrict returned goals by `GoalStatus`.
///
/// Returns `Ok(Vec<Goal>)` on success or an `AppError` on failure.
#[cfg_attr(not(target_arch = "wasm32"), uniffi::export)]
pub fn list_goals(
    archive_path: String,
    statuses: Option<Vec<GoalStatus>>,
) -> Result<Vec<Goal>, AppError> {
    goals::list_goals(Path::new(&archive_path), statuses.as_deref())
}

/// Return goals that are currently trashed
///
/// Returns `Ok(Vec<Goal>)` on success or an `AppError` on failure.
#[cfg_attr(not(target_arch = "wasm32"), uniffi::export)]
pub fn list_trash(archive_path: String) -> Result<Vec<Goal>, AppError> {
    goals::list_trash(Path::new(&archive_path))
}

/// Search goals by `query` in the archive at `archive_path`.
///
/// - `query`: text to search for in goal names/metadata.
/// - `is_reward`: optional filter limiting results to reward/non-reward goals.
/// - `statuses`: optional list of `GoalStatus` values to include. defaults to TODO, DOING
/// - `sort_by_recent`: optional bool, defaults to true.
///
/// Returns matching goals or an `AppError` on failure.
#[cfg_attr(not(target_arch = "wasm32"), uniffi::export)]
pub fn search_goals(
    archive_path: String,
    query: String,
    is_reward: Option<bool>,
    statuses: Option<Vec<GoalStatus>>,
    sort_by_recent: Option<bool>,
) -> Result<Vec<Goal>, AppError> {
    goals::search_goals(
        Path::new(&archive_path),
        &query,
        is_reward,
        statuses.as_deref(),
        sort_by_recent.unwrap_or(true),
    )
}

/// Add a new goal
///
/// - `name`: the goal name.
/// - `is_reward`: whether this goal is considered a reward.
/// - `commands`: associated commands for the goal.
///
/// Returns the created `Goal` or an `AppError` on failure.
#[cfg_attr(not(target_arch = "wasm32"), uniffi::export)]
pub fn add_goal(
    archive_path: String,
    name: String,
    is_reward: bool,
    commands: Vec<String>,
    quantity_name: Option<String>,
) -> Result<Goal, AppError> {
    goals::add_goal(
        Path::new(&archive_path),
        &name,
        is_reward,
        commands,
        quantity_name,
    )
}

/// Retrieve the note content for the goal identified by `goal_id`.
///
/// Returns the note text as `String` or an `AppError` if retrieval fails.
#[cfg_attr(not(target_arch = "wasm32"), uniffi::export)]
pub fn get_note(archive_path: String, goal_id: u64) -> Result<String, AppError> {
    notes::get_note(Path::new(&archive_path), goal_id)
}

/// Replace the note content for the goal `goal_id` with `content`.
///
/// Returns `Ok(true)` on success or an `AppError` on failure.
#[cfg_attr(not(target_arch = "wasm32"), uniffi::export)]
pub fn edit_note(
    archive_path: String,
    goal_id: u64,
    content: String,
) -> Result<bool, AppError> {
    notes::edit_note(Path::new(&archive_path), goal_id, &content)?;
    Ok(true)
}

/// Update the `status` of the goal identified by `goal_id`.
///
/// Returns the updated `Goal` on success or an `AppError` on failure.
#[cfg_attr(not(target_arch = "wasm32"), uniffi::export)]
pub fn set_goal_status(
    archive_path: String,
    goal_id: u64,
    status: GoalStatus,
) -> Result<Goal, AppError> {
    goals::set_goal_status(Path::new(&archive_path), goal_id, status)
}

/// Mark a goal as trashed or untrashed.
///
/// - `trashed`: `true` to move the goal to trash, `false` to restore it.
///
/// Returns the updated `Goal` or an `AppError` on failure.
#[cfg_attr(not(target_arch = "wasm32"), uniffi::export)]
pub fn set_goal_trashed(
    archive_path: String,
    goal_id: u64,
    trashed: bool,
) -> Result<Goal, AppError> {
    goals::set_goal_trashed(Path::new(&archive_path), goal_id, trashed)
}

/// Add a session for the specified goal and return a `SessionView`.
///
/// - `start_ts_secs`: Unix timestamp (seconds) for session start.
/// - `duration_secs`: duration of the session in seconds.
/// - `is_reward`: whether the session is tied to a reward goal.
///
/// Returns the created `SessionView` or an `AppError` on failure.
#[cfg_attr(not(target_arch = "wasm32"), uniffi::export)]
pub fn add_session(
    archive_path: String,
    goal_id: u64,
    goal_name: String,
    start_ts_secs: i64,
    duration_secs: u32,
    is_reward: bool,
    quantity: Option<u32>,
) -> Result<SessionView, AppError> {
    let start_at = Utc
        .timestamp_opt(start_ts_secs, 0)
        .single()
        .ok_or_else(|| AppError::InvalidInput {
            detail: format!("invalid start_ts_secs: {start_ts_secs}"),
        })?;

    session_graph::add_session(
        Path::new(&archive_path),
        goal_id,
        &goal_name,
        start_at,
        duration_secs,
        is_reward,
        quantity,
    )
    .map(SessionView::from)
}

/// List sessions that occurred on the given ISO date (YYYY-MM-DD).
///
/// - `date_iso`: date in `YYYY-MM-DD` format.
///
/// Returns a vector of `SessionView` or an `AppError` on failure.
#[cfg_attr(not(target_arch = "wasm32"), uniffi::export)]
pub fn list_day_sessions(
    archive_path: String,
    date_iso: String,
) -> Result<Vec<SessionView>, AppError> {
    let date = NaiveDate::parse_from_str(&date_iso, "%Y-%m-%d").map_err(|e| {
        AppError::InvalidInput {
            detail: format!("date_iso must be YYYY-MM-DD: {e}"),
        }
    })?;

    let sessions = session_graph::list_day_sessions(Path::new(&archive_path), date)?;
    Ok(sessions.into_iter().map(SessionView::from).collect())
}

/// List sessions between two dates (inclusive).
///
/// - `start_date_iso`: optional start date in `YYYY-MM-DD` format (defaults to 7 days ago).
/// - `end_date_iso`: optional end date in `YYYY-MM-DD` format (defaults to today).
///
/// Returns a vector of `SessionView` or an `AppError` on failure.
#[cfg_attr(not(target_arch = "wasm32"), uniffi::export)]
pub fn list_sessions_between_dates(
    archive_path: String,
    start_date_iso: Option<String>,
    end_date_iso: Option<String>,
) -> Result<Vec<SessionView>, AppError> {
    let sessions = session_graph::list_sessions_between_dates(
        Path::new(&archive_path),
        start_date_iso.as_deref(),
        end_date_iso.as_deref(),
    )?;
    Ok(sessions.into_iter().map(SessionView::from).collect())
}
