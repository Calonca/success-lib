use crate::storage_io::StorageIoError;

#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Error))]
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("I/O error: {detail}")]
    Io { detail: String },

    #[error("Storage unavailable")]
    StorageUnavailable,

    #[error("Invalid path: {detail}")]
    InvalidPath { detail: String },

    #[error("{resource} not found: {id}")]
    NotFound { resource: String, id: String },

    #[error("Invalid input: {detail}")]
    InvalidInput { detail: String },

    #[error("Parse error: {detail}")]
    Parse { detail: String },
}

impl From<StorageIoError> for AppError {
    fn from(e: StorageIoError) -> Self {
        match e {
            StorageIoError::StorageUnavailable => AppError::StorageUnavailable,
            StorageIoError::InvalidUtf8Path => AppError::InvalidPath {
                detail: "invalid UTF-8 in path".into(),
            },
            StorageIoError::Io(io_err) => AppError::Io {
                detail: io_err.to_string(),
            },
        }
    }
}

impl From<serde_yaml::Error> for AppError {
    fn from(e: serde_yaml::Error) -> Self {
        AppError::Parse {
            detail: e.to_string(),
        }
    }
}
