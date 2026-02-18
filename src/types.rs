//! Types used by the Success FFI surface.
//!
//! This module defines the primary domain types exposed to foreign
//! language bindings: `Goal`, `Session`, and their supporting enums.
//! These types are serializable and annotated for `uniffi` where needed.
use chrono::{Local, TimeZone, Utc};
use serde::{Deserialize, Serialize};

/// The semantic kind of a session.
///
/// - `Goal`: a session associated with a normal goal.
/// - `Reward`: a session associated with a reward goal.
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionKind {
    Goal,
    Reward,
}

/// The current status of a `Goal`.
///
/// - `TODO`: goal not yet started.
/// - `DOING`: goal in progress.
/// - `DONE`: goal completed.
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GoalStatus {
    #[default]
    TODO,
    DOING,
    DONE,
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
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    #[serde(default)]
    pub quantity_name: Option<String>,
}

/// A recorded session entry.
///
/// - `id`: unique string identifier for the session.
/// - `name`: human-friendly session name.
/// - `goal_id`: the associated goal's id.
/// - `kind`: whether this was a `Goal` or `Reward` session.
/// - `start_at` / `end_at`: Unix timestamps in seconds (UTC).
/// - `quantity`: optional quantity recorded during the session.
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub name: String,
    pub goal_id: u64,
    pub kind: SessionKind,
    #[serde(default)]
    pub quantity: Option<u32>,
    #[serde(default)]
    pub start_at: i64,
    #[serde(default)]
    pub end_at: i64,
}

/// Convert a Unix-seconds timestamp to an ISO date string (`YYYY-MM-DD`)
/// in the **local** timezone.
pub fn timestamp_to_date_iso(ts: i64) -> String {
    let dt = Utc
        .timestamp_opt(ts, 0)
        .single()
        .expect("valid timestamp");
    dt.with_timezone(&Local)
        .format("%Y-%m-%d")
        .to_string()
}
