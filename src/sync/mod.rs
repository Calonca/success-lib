//! Offline-first synchronization of an archive with a remote store.
//!
//! - [`merge`]: pure three-way merge functions per file type.
//! - [`state`]: local sync bookkeeping (`.sync/` revisions and base snapshots).
//! - [`remote`]: the [`remote::RemoteStore`] protocol and an in-memory fake.

pub mod engine;
pub mod merge;
pub mod remote;
pub mod state;
pub mod supabase;
