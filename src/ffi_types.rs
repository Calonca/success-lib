use crate::types::{Session, SessionKind};

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum AppError {
    // Use `detail` to avoid clashing with Kotlin's built-in `Exception.message` property.
    #[error("Error: {detail}")]
    Generic { detail: String },
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::Generic {
            detail: e.to_string(),
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct SessionView {
    pub id: String,
    pub name: String,
    pub goal_id: u64,
    pub kind: SessionKind,
    pub quantity: Option<u32>,
    pub start_at: i64,
    pub end_at: i64,
}

impl From<Session> for SessionView {
    fn from(value: Session) -> Self {
        SessionView {
            id: value.id,
            name: value.name,
            goal_id: value.goal_id,
            kind: value.kind,
            quantity: value.quantity,
            start_at: value.start_at.timestamp(),
            end_at: value.end_at.timestamp(),
        }
    }
}
