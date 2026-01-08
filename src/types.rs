//! Types used by the Success FFI surface.
//!
//! This module defines the primary domain types exposed to foreign
//! language bindings: `Goal`, `Session`, and their supporting enums.
//! These types are serializable and annotated for `uniffi` where needed.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The semantic kind of a session.
///
/// - `Goal`: a session associated with a normal goal.
/// - `Reward`: a session associated with a reward goal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
pub enum SessionKind {
    Goal,
    Reward,
}

/// The current status of a `Goal`.
///
/// - `TODO`: goal not yet started.
/// - `DOING`: goal in progress.
/// - `DONE`: goal completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
pub enum GoalStatus {
    TODO,
    DOING,
    DONE,
}

impl Default for GoalStatus {
    fn default() -> Self {
        GoalStatus::TODO
    }
}

/// A goal managed in the archive.
///
/// Fields:
/// - `id`: unique numeric identifier.
/// - `name`: human-readable name.
/// - `is_reward`: whether the goal is a reward type.
/// - `commands`: optional associated commands.
/// - `status`: current `GoalStatus`.
/// - `trashed`: whether the goal is in the trash bin.
#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct Goal {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub is_reward: bool,
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default)]
    pub status: GoalStatus,
    #[serde(default)]
    pub trashed: bool,
}

/// A recorded session entry.
///
/// - `id`: unique string identifier for the session.
/// - `name`: human-friendly session name.
/// - `goal_id`: the associated goal's id.
/// - `kind`: whether this was a `Goal` or `Reward` session.
/// - `start_at` / `end_at`: timestamps in UTC stored as seconds since epoch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub name: String,
    pub goal_id: u64,
    pub kind: SessionKind,
    #[serde(default = "utc_now_ts", with = "chrono::serde::ts_seconds")]
    pub start_at: DateTime<Utc>,
    #[serde(default = "utc_now_ts", with = "chrono::serde::ts_seconds")]
    pub end_at: DateTime<Utc>,
}

/// Helper to provide a default UTC timestamp for serde defaults.
#[doc(hidden)]
fn utc_now_ts() -> DateTime<Utc> {
    Utc::now()
}
