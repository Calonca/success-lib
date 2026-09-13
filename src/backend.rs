//! Pluggable storage backends.
//!
//! All persistence in the crate goes through the [`StorageBackend`] trait,
//! which models the archive as a flat map of archive-relative, forward-slash
//! paths (e.g. `"goals.yaml"`, `"graphs/2026-08-09.mmd"`) to UTF-8 contents.
//!
//! Implementations:
//! - [`FsBackend`]: filesystem, rooted at the archive directory (native).
//! - [`LocalStorageBackend`]: browser `localStorage` (wasm32).
//! - [`MemoryBackend`]: in-memory map for tests.

use crate::storage_io::{StorageIoError, StorageIoResult};

/// Storage abstraction over archive-relative paths.
pub trait StorageBackend {
    /// Read the contents at `path`, or `None` if it does not exist.
    fn read(&self, path: &str) -> StorageIoResult<Option<String>>;
    /// Write `content` at `path`, creating any missing parents.
    fn write(&self, path: &str, content: &str) -> StorageIoResult<()>;
    /// List all stored paths starting with `prefix`, sorted.
    fn list(&self, prefix: &str) -> StorageIoResult<Vec<String>>;
    /// Delete the entry at `path`. Deleting a missing path is not an error.
    fn delete(&self, path: &str) -> StorageIoResult<()>;
    /// Create the base archive layout (goals file, directories) if missing.
    fn ensure_structure(&self) -> StorageIoResult<()>;
}

/// In-memory backend for tests.
#[derive(Default)]
pub struct MemoryBackend {
    entries: std::cell::RefCell<std::collections::BTreeMap<String, String>>,
}

impl MemoryBackend {
    pub fn new() -> Self {
        Self::default()
    }
}

impl StorageBackend for MemoryBackend {
    fn read(&self, path: &str) -> StorageIoResult<Option<String>> {
        Ok(self.entries.borrow().get(path).cloned())
    }

    fn write(&self, path: &str, content: &str) -> StorageIoResult<()> {
        self.entries
            .borrow_mut()
            .insert(path.to_string(), content.to_string());
        Ok(())
    }

    fn list(&self, prefix: &str) -> StorageIoResult<Vec<String>> {
        Ok(self
            .entries
            .borrow()
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect())
    }

    fn delete(&self, path: &str) -> StorageIoResult<()> {
        self.entries.borrow_mut().remove(path);
        Ok(())
    }

    fn ensure_structure(&self) -> StorageIoResult<()> {
        let mut entries = self.entries.borrow_mut();
        if !entries.contains_key("goals.yaml") {
            entries.insert("goals.yaml".into(), "[]".into());
        }
        Ok(())
    }
}

/// Filesystem backend rooted at the archive directory (native platforms).
#[cfg(not(target_arch = "wasm32"))]
pub struct FsBackend {
    root: std::path::PathBuf,
}

#[cfg(not(target_arch = "wasm32"))]
impl FsBackend {
    pub fn new(root: &std::path::Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }

    fn resolve(&self, path: &str) -> std::path::PathBuf {
        self.root.join(path)
    }

    fn collect_files(
        &self,
        dir: &std::path::Path,
        out: &mut Vec<String>,
    ) -> StorageIoResult<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                self.collect_files(&path, out)?;
            } else {
                let relative = path
                    .strip_prefix(&self.root)
                    .map_err(|_| StorageIoError::InvalidUtf8Path)?;
                let relative = relative.to_str().ok_or(StorageIoError::InvalidUtf8Path)?;
                out.push(relative.replace(std::path::MAIN_SEPARATOR, "/"));
            }
        }
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl StorageBackend for FsBackend {
    fn read(&self, path: &str) -> StorageIoResult<Option<String>> {
        let full = self.resolve(path);
        if !full.exists() {
            return Ok(None);
        }
        Ok(Some(std::fs::read_to_string(full)?))
    }

    fn write(&self, path: &str, content: &str) -> StorageIoResult<()> {
        let full = self.resolve(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Write-then-rename so a crash mid-write never leaves a truncated
        // file behind (renames within a directory are atomic).
        let file_name = full
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or(StorageIoError::InvalidUtf8Path)?;
        let tmp = full.with_file_name(format!("{file_name}.tmp"));
        std::fs::write(&tmp, content)?;
        std::fs::rename(&tmp, &full)?;
        Ok(())
    }

    fn list(&self, prefix: &str) -> StorageIoResult<Vec<String>> {
        if !self.root.exists() {
            return Ok(vec![]);
        }
        let mut all = Vec::new();
        self.collect_files(&self.root.clone(), &mut all)?;
        let mut matching: Vec<String> = all
            .into_iter()
            .filter(|p| p.starts_with(prefix))
            .collect();
        matching.sort();
        Ok(matching)
    }

    fn delete(&self, path: &str) -> StorageIoResult<()> {
        match std::fs::remove_file(self.resolve(path)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn ensure_structure(&self) -> StorageIoResult<()> {
        std::fs::create_dir_all(&self.root)?;
        std::fs::create_dir_all(self.root.join("graphs"))?;
        std::fs::create_dir_all(self.root.join("notes"))?;
        let goals_path = self.root.join("goals.yaml");
        if !goals_path.exists() {
            std::fs::write(goals_path, "[]")?;
        }
        Ok(())
    }
}

/// Browser `localStorage` backend (wasm32).
///
/// Keys use the historical scheme `"{archive}_{path with '/' -> '__'}"` so
/// existing stored data keeps working.
#[cfg(target_arch = "wasm32")]
pub struct LocalStorageBackend {
    prefix: String,
}

#[cfg(target_arch = "wasm32")]
impl LocalStorageBackend {
    pub fn new(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
        }
    }

    fn storage() -> StorageIoResult<web_sys::Storage> {
        let window = web_sys::window().ok_or(StorageIoError::StorageUnavailable)?;
        window
            .local_storage()
            .map_err(|_| StorageIoError::StorageUnavailable)?
            .ok_or(StorageIoError::StorageUnavailable)
    }

    fn key_for(&self, path: &str) -> String {
        let normalized = path.trim_start_matches('/').replace('/', "__");
        format!("{}_{normalized}", self.prefix)
    }

    fn path_for(&self, key: &str) -> Option<String> {
        let head = format!("{}_", self.prefix);
        key.strip_prefix(&head).map(|p| p.replace("__", "/"))
    }
}

#[cfg(target_arch = "wasm32")]
impl StorageBackend for LocalStorageBackend {
    fn read(&self, path: &str) -> StorageIoResult<Option<String>> {
        let storage = Self::storage()?;
        storage
            .get_item(&self.key_for(path))
            .map_err(|_| StorageIoError::StorageUnavailable)
    }

    fn write(&self, path: &str, content: &str) -> StorageIoResult<()> {
        let storage = Self::storage()?;
        storage
            .set_item(&self.key_for(path), content)
            .map_err(|_| StorageIoError::StorageUnavailable)
    }

    fn list(&self, prefix: &str) -> StorageIoResult<Vec<String>> {
        let storage = Self::storage()?;
        let len = storage
            .length()
            .map_err(|_| StorageIoError::StorageUnavailable)?;
        let mut paths = Vec::new();
        for i in 0..len {
            let Some(key) = storage
                .key(i)
                .map_err(|_| StorageIoError::StorageUnavailable)?
            else {
                continue;
            };
            if let Some(path) = self.path_for(&key) {
                if path.starts_with(prefix) {
                    paths.push(path);
                }
            }
        }
        paths.sort();
        Ok(paths)
    }

    fn delete(&self, path: &str) -> StorageIoResult<()> {
        let storage = Self::storage()?;
        storage
            .remove_item(&self.key_for(path))
            .map_err(|_| StorageIoError::StorageUnavailable)
    }

    fn ensure_structure(&self) -> StorageIoResult<()> {
        if self.read("goals.yaml")?.is_none() {
            self.write("goals.yaml", "[]")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_roundtrip() {
        let b = MemoryBackend::new();
        b.write("goals.yaml", "[]").unwrap();
        assert_eq!(b.read("goals.yaml").unwrap(), Some("[]".to_string()));
    }

    #[test]
    fn memory_read_missing_is_none() {
        let b = MemoryBackend::new();
        assert_eq!(b.read("missing.md").unwrap(), None);
    }

    #[test]
    fn memory_list_by_prefix_sorted() {
        let b = MemoryBackend::new();
        b.write("graphs/2026-08-09.mmd", "b").unwrap();
        b.write("graphs/2026-08-08.mmd", "a").unwrap();
        b.write("notes/goal_1.md", "n").unwrap();
        assert_eq!(
            b.list("graphs/").unwrap(),
            vec![
                "graphs/2026-08-08.mmd".to_string(),
                "graphs/2026-08-09.mmd".to_string()
            ]
        );
    }

    #[test]
    fn memory_delete_is_idempotent() {
        let b = MemoryBackend::new();
        b.write("notes/goal_1.md", "hi").unwrap();
        b.delete("notes/goal_1.md").unwrap();
        assert_eq!(b.read("notes/goal_1.md").unwrap(), None);
        b.delete("notes/goal_1.md").unwrap();
    }

    #[test]
    fn memory_ensure_structure_creates_goals_once() {
        let b = MemoryBackend::new();
        b.ensure_structure().unwrap();
        assert_eq!(b.read("goals.yaml").unwrap(), Some("[]".to_string()));
        b.write("goals.yaml", "custom").unwrap();
        b.ensure_structure().unwrap();
        assert_eq!(b.read("goals.yaml").unwrap(), Some("custom".to_string()));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn fs_roundtrip_list_delete() {
        let temp = tempfile::tempdir().unwrap();
        let b = FsBackend::new(temp.path());
        b.ensure_structure().unwrap();
        b.write("graphs/2026-08-09.mmd", "stateDiagram-v2\n").unwrap();
        b.write("notes/goal_1.md", "note\n").unwrap();

        assert_eq!(
            b.read("graphs/2026-08-09.mmd").unwrap(),
            Some("stateDiagram-v2\n".to_string())
        );
        assert_eq!(
            b.list("graphs/").unwrap(),
            vec!["graphs/2026-08-09.mmd".to_string()]
        );
        let all = b.list("").unwrap();
        assert!(all.contains(&"goals.yaml".to_string()));
        assert!(all.contains(&"notes/goal_1.md".to_string()));

        b.delete("notes/goal_1.md").unwrap();
        assert_eq!(b.read("notes/goal_1.md").unwrap(), None);
        b.delete("notes/goal_1.md").unwrap();
    }
}
