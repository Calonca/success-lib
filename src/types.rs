use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
pub enum SessionKind {
    Goal,
    Reward,
}

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

fn utc_now_ts() -> DateTime<Utc> {
    Utc::now()
}
