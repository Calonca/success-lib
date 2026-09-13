//! The remote document-store protocol used for sync.
//!
//! Any server that can store `(path, content, revision)` rows with an
//! atomic compare-and-set on `revision` can implement [`RemoteStore`].

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SyncError {
    /// The remote revision no longer matches the expected base revision
    /// (another device synced concurrently).
    #[error("remote revision conflict")]
    Conflict,
    #[error("http error {status}: {detail}")]
    Http { status: u16, detail: String },
    #[error("network error: {detail}")]
    Network { detail: String },
    #[error("protocol error: {detail}")]
    Protocol { detail: String },
}

/// A document listed on the remote.
#[derive(Debug, Clone)]
pub struct RemoteEntry {
    pub path: String,
    pub revision: i64,
}

/// A document fetched from the remote.
#[derive(Debug, Clone)]
pub struct RemoteDoc {
    pub content: String,
    pub revision: i64,
}

/// Minimal async document store with optimistic concurrency.
pub trait RemoteStore {
    /// List every document path with its current revision.
    fn list(&self) -> impl std::future::Future<Output = Result<Vec<RemoteEntry>, SyncError>>;
    /// Fetch a document, or `None` if it does not exist.
    fn get(
        &self,
        path: &str,
    ) -> impl std::future::Future<Output = Result<Option<RemoteDoc>, SyncError>>;
    /// Store a document. `base_revision` is the revision this write is based
    /// on (`None` = the document is expected to be new). Returns the new
    /// revision, or [`SyncError::Conflict`] if the remote moved on.
    fn put(
        &self,
        path: &str,
        content: &str,
        base_revision: Option<i64>,
    ) -> impl std::future::Future<Output = Result<i64, SyncError>>;
}

/// In-memory [`RemoteStore`] for tests.
#[derive(Default)]
pub struct MemoryRemote {
    docs: std::cell::RefCell<std::collections::BTreeMap<String, (String, i64)>>,
}

impl MemoryRemote {
    pub fn new() -> Self {
        Self::default()
    }

    /// All stored paths (test helper).
    pub fn paths(&self) -> Vec<String> {
        self.docs.borrow().keys().cloned().collect()
    }
}

impl RemoteStore for MemoryRemote {
    async fn list(&self) -> Result<Vec<RemoteEntry>, SyncError> {
        Ok(self
            .docs
            .borrow()
            .iter()
            .map(|(path, (_, revision))| RemoteEntry {
                path: path.clone(),
                revision: *revision,
            })
            .collect())
    }

    async fn get(&self, path: &str) -> Result<Option<RemoteDoc>, SyncError> {
        Ok(self.docs.borrow().get(path).map(|(content, revision)| {
            RemoteDoc {
                content: content.clone(),
                revision: *revision,
            }
        }))
    }

    async fn put(
        &self,
        path: &str,
        content: &str,
        base_revision: Option<i64>,
    ) -> Result<i64, SyncError> {
        let mut docs = self.docs.borrow_mut();
        let current = docs.get(path).map(|(_, rev)| *rev);
        if current != base_revision {
            return Err(SyncError::Conflict);
        }
        let next = current.unwrap_or(0) + 1;
        docs.insert(path.to_string(), (content.to_string(), next));
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_new_get_list() {
        pollster::block_on(async {
            let remote = MemoryRemote::new();
            let rev = remote.put("goals.yaml", "[]", None).await.unwrap();
            assert_eq!(rev, 1);

            let doc = remote.get("goals.yaml").await.unwrap().unwrap();
            assert_eq!(doc.content, "[]");
            assert_eq!(doc.revision, 1);

            let entries = remote.list().await.unwrap();
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].path, "goals.yaml");

            assert!(remote.get("missing").await.unwrap().is_none());
        });
    }

    #[test]
    fn put_with_stale_base_conflicts() {
        pollster::block_on(async {
            let remote = MemoryRemote::new();
            remote.put("goals.yaml", "v1", None).await.unwrap();

            // second writer bases off nothing / stale revision
            assert!(matches!(
                remote.put("goals.yaml", "v2", None).await,
                Err(SyncError::Conflict)
            ));
            assert!(matches!(
                remote.put("goals.yaml", "v2", Some(99)).await,
                Err(SyncError::Conflict)
            ));

            let rev = remote.put("goals.yaml", "v2", Some(1)).await.unwrap();
            assert_eq!(rev, 2);
        });
    }
}
