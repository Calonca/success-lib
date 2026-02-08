use std::path::Path;

use thiserror::Error;

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

#[cfg(target_arch = "wasm32")]
fn local_storage() -> StorageIoResult<web_sys::Storage> {
    let window = web_sys::window().ok_or(StorageIoError::StorageUnavailable)?;
    window
        .local_storage()
        .map_err(|_| StorageIoError::StorageUnavailable)?
        .ok_or(StorageIoError::StorageUnavailable)
}

#[cfg(target_arch = "wasm32")]
fn storage_key(prefix: &Path, path: &Path) -> StorageIoResult<String> {
    let prefix = prefix.to_str().ok_or(StorageIoError::InvalidUtf8Path)?;
    let relative = path.strip_prefix(prefix).unwrap_or(path);
    let path = relative.to_str().ok_or(StorageIoError::InvalidUtf8Path)?;
    let normalized = path.trim_start_matches('/').replace('/', "__");
    Ok(format!("{prefix}_{normalized}"))
}

#[cfg(target_arch = "wasm32")]
pub fn read_to_string(archive: &Path, path: &Path) -> StorageIoResult<Option<String>> {
    let storage = local_storage()?;
    let key = storage_key(archive, path)?;
    Ok(storage
        .get_item(&key)
        .map_err(|_| StorageIoError::StorageUnavailable)?)
}

#[cfg(target_arch = "wasm32")]
pub fn write_string(archive: &Path, path: &Path, content: &str) -> StorageIoResult<()> {
    let storage = local_storage()?;
    let key = storage_key(archive, path)?;
    storage
        .set_item(&key, content)
        .map_err(|_| StorageIoError::StorageUnavailable)?;
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn read_to_string(_archive: &Path, path: &Path) -> StorageIoResult<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(std::fs::read_to_string(path)?))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn write_string(_archive: &Path, path: &Path, content: &str) -> StorageIoResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)?;
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn ensure_archive_structure(archive: &Path) -> StorageIoResult<()> {
    std::fs::create_dir_all(archive)?;
    std::fs::create_dir_all(archive.join("graphs"))?;
    std::fs::create_dir_all(archive.join("notes"))?;
    let goals_path = archive.join("goals.yaml");
    if !goals_path.exists() {
        std::fs::write(goals_path, "[]")?;
    }
    Ok(())
}

#[cfg(target_arch = "wasm32")]
pub fn ensure_archive_structure(archive: &Path) -> StorageIoResult<()> {
    let storage = local_storage()?;
    let key = storage_key(archive, Path::new("goals.yaml"))?;
    if storage
        .get_item(&key)
        .map_err(|_| StorageIoError::StorageUnavailable)?
        .is_none()
    {
        storage
            .set_item(&key, "[]")
            .map_err(|_| StorageIoError::StorageUnavailable)?;
    }
    Ok(())
}
