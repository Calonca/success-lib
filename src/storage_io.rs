use std::path::Path;

use thiserror::Error;

use crate::backend::StorageBackend;

pub type StorageIoResult<T> = Result<T, StorageIoError>;

#[derive(Debug, Error)]
pub enum StorageIoError {
    #[error("storage unavailable")]
    StorageUnavailable,
    #[error("invalid utf-8 in path")]
    InvalidUtf8Path,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// The default storage backend for the archive on this platform.
#[cfg(not(target_arch = "wasm32"))]
pub fn backend_for(archive: &Path) -> crate::backend::FsBackend {
    crate::backend::FsBackend::new(archive)
}

/// The default storage backend for the archive on this platform.
#[cfg(target_arch = "wasm32")]
pub fn backend_for(archive: &Path) -> crate::backend::LocalStorageBackend {
    let prefix = archive.to_str().unwrap_or_default();
    crate::backend::LocalStorageBackend::new(prefix)
}

/// Convert an absolute (or already relative) path into the archive-relative
/// form used by [`StorageBackend`].
fn relative_path(archive: &Path, path: &Path) -> StorageIoResult<String> {
    let relative = path.strip_prefix(archive).unwrap_or(path);
    let relative = relative.to_str().ok_or(StorageIoError::InvalidUtf8Path)?;
    Ok(relative
        .trim_start_matches('/')
        .replace(std::path::MAIN_SEPARATOR, "/"))
}

pub fn read_to_string(archive: &Path, path: &Path) -> StorageIoResult<Option<String>> {
    backend_for(archive).read(&relative_path(archive, path)?)
}

pub fn write_string(archive: &Path, path: &Path, content: &str) -> StorageIoResult<()> {
    backend_for(archive).write(&relative_path(archive, path)?, content)
}

pub fn ensure_archive_structure(archive: &Path) -> StorageIoResult<()> {
    backend_for(archive).ensure_structure()
}
