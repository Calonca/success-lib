# Design: Generic Storage Backend + Remote Sync

**Date:** 2026-08-09
**Status:** Draft — pending approval

## Problem

success-lib persists everything through `src/storage_io.rs`, a pair of cfg-gated
free functions (filesystem on native, `localStorage` on wasm). There is no way to
plug in a different storage backend, and no way to synchronize an archive between
devices — a core promise of the project ("Share a single goal archive across
devices").

## Goals

1. A **generic backend abstraction** so storage implementations are pluggable
   rather than hard-wired by `cfg`.
2. A **remote-database-backed sync** so the same archive can be used from
   multiple devices (desktop, Android, web) without data loss.

## Non-goals

- Real-time / live sync (explicit `sync()` calls only).
- Multi-user collaboration, auth flows, permissions (single user, one API key).
- Changing the human-readable Markdown/YAML file formats.

## Constraints

- Must work on native (UniFFI: Kotlin/Swift/desktop) **and** wasm32 (web).
  Networking on wasm is fetch-based and async-only.
- Existing public API must keep working unchanged, offline, synchronously.
- Users own their data: the local archive remains the source of truth.

---

## 1. `StorageBackend` trait

Replace the cfg-gated free functions in `storage_io.rs` with a trait over
**archive-relative string paths** (`"goals.yaml"`, `"graphs/2026-08-09.mmd"`,
`"notes/goal_1.md"`):

```rust
pub trait StorageBackend {
    fn read(&self, path: &str) -> Result<Option<String>, StorageIoError>;
    fn write(&self, path: &str, content: &str) -> Result<(), StorageIoError>;
    fn list(&self, prefix: &str) -> Result<Vec<String>, StorageIoError>;
    fn ensure_structure(&self) -> Result<(), StorageIoError>;
}
```

Implementations:

| Backend | Target | Notes |
|---|---|---|
| `FsBackend` | native | current filesystem behavior, rooted at the archive dir |
| `LocalStorageBackend` | wasm32 | current `localStorage` behavior, same key scheme |
| `MemoryBackend` | tests | `HashMap<String, String>`; also reusable for sync tests |

`list(prefix)` is new. On native it walks the directory; on wasm it scans
`localStorage` keys by prefix. Sync needs it to enumerate `graphs/` and `notes/`.

**Compatibility:** `storage_io::read_to_string` / `write_string` /
`ensure_archive_structure` keep their signatures and delegate to the platform
default backend, so `goals.rs`, `notes.rs`, `session_graph.rs`, and the whole
FFI surface are untouched by this refactor. Domain modules may later take a
`&dyn StorageBackend` if needed, but that is not required for this project.

## 2. Remote protocol + `SupabaseStore`

A minimal async document-store trait — the "generic HTTP KV" protocol. Any
server that can store `(path, content, revision)` rows can implement it:

```rust
pub struct RemoteEntry { pub path: String, pub revision: i64 }
pub struct RemoteDoc  { pub content: String, pub revision: i64 }

pub trait RemoteStore {
    async fn list(&self) -> Result<Vec<RemoteEntry>, SyncError>;
    async fn get(&self, path: &str) -> Result<Option<RemoteDoc>, SyncError>;
    /// Optimistic concurrency: fails with `SyncError::Conflict` if the remote
    /// revision no longer matches `base_revision` (None = expect absent).
    async fn put(&self, path: &str, content: &str, base_revision: Option<i64>)
        -> Result<i64, SyncError>;
}
```

**First implementation: `SupabaseStore`** over PostgREST (plain HTTPS, works on
native and wasm via `reqwest`). One table:

```sql
create table documents (
  archive_id text not null,
  path       text not null,
  content    text not null,
  revision   bigint not null default 1,
  updated_at timestamptz not null default now(),
  primary key (archive_id, path)
);
```

- `put` uses a conditional `PATCH ...?revision=eq.{base}` (checking affected
  rows) or `POST` for new rows, bumping `revision`. A mismatch surfaces as
  `SyncError::Conflict`, which the engine resolves by re-merging.
- Config = `remote_url`, `api_key`, `archive_id` — passed per call, matching the
  stateless `archive_path: String` style of the existing API.
- The SQL setup script ships in the repo (`docs/supabase-setup.sql`) with README
  instructions.

## 3. Sync engine (offline-first, three-way merge)

New `src/sync/` module. Local file `.sync/state.json` (stored via the local
backend, excluded from sync) records per path: the remote `revision` and a
content hash from the last successful sync. That hash is the **merge base**
indicator.

Per-file algorithm on `sync()`:

| Local changed | Remote changed | Action |
|---|---|---|
| no | no | nothing |
| yes | no | push |
| no | yes | pull |
| yes | yes | content-aware merge → write locally → push |

"Local changed" = current content hash ≠ last-synced hash.
"Remote changed" = remote revision ≠ last-synced revision.
Files new on one side are simply pushed/pulled. After each transfer the state
file is updated. A `Conflict` from `put` (someone synced concurrently) triggers
a re-fetch and re-merge of that file, retried a bounded number of times.

### Merge rules by file type

- **`graphs/YYYY-MM-DD.mmd` (sessions):** parse both sides, take the **union**
  of sessions keyed by `(goal_id, start_at, end_at, kind)`, sort by `start_at`,
  regenerate `sess_N`/`rew_N` ids (they are per-file counters already
  regenerated on save). Never loses a session.
- **`goals.yaml`:** merge per goal id. A goal present on only one side is kept.
  For a goal on both sides: if equal, done; otherwise prefer the **local**
  version (deterministic; without per-field timestamps there is no better
  signal, and status flips like TODO→DOING are low-stakes).
- **`notes/goal_N.md`:** if the sides differ, and only one differs from the
  last-synced content, take it; if both changed, **concatenate** both versions
  separated by a `---` conflict divider with a short header line. Nothing is
  silently dropped; the file stays valid Markdown the user can tidy up.

### Goal id collisions

`next_goal_id()` is `max + 1`, so two devices adding goals offline will mint the
same id for different goals. Fix: `add_goal` generates a **random `u64`** id
(retrying on the vanishingly unlikely local collision). Existing sequential ids
remain valid. The goals merge additionally detects the legacy case (same id,
both sides new since base, different names) and reassigns the local goal a fresh
id, rewriting its note path and its sessions' `goal_id` references.

## 4. Public API additions

Existing functions are unchanged. New exports:

```rust
/// Bidirectional sync of the archive with the remote store.
pub async fn sync(
    archive_path: String,
    remote_url: String,
    api_key: String,
    archive_id: String,
) -> Result<SyncReport, AppError>;

pub struct SyncReport { pub pushed: u32, pub pulled: u32, pub merged: u32 }
```

- **Native:** exported as a UniFFI async fn (suspend fn in Kotlin, async in
  Swift). `reqwest` needs a reactor, so a small lazily-initialized tokio runtime
  lives inside the crate.
- **wasm:** async fn via `wasm-bindgen-futures`; `reqwest` uses fetch.
- New `AppError::Sync { detail }` variant carries sync failures across FFI.

New dependencies: `reqwest` (json, rustls), `tokio` (native only, rt), `sha2`
(content hashes), `rand` (goal ids), `wasm-bindgen-futures` (wasm only).

## 5. Error handling

- Network/HTTP errors abort `sync()` with `AppError::Sync`; the local archive is
  never left partially merged — each file is written locally only after its
  merge completes, and per-file state is committed after that file's push/pull
  succeeds, so a crash mid-sync just means some files sync next time.
- `revision` optimistic-concurrency prevents lost updates when two devices sync
  simultaneously.
- Malformed remote content falls back to treating the file as conflicting
  (pull-side parse errors keep the local version and report it in `SyncReport`).

## 6. Testing

- Unit tests for each merge rule (sessions union, goals three-way, notes
  concatenation, id-collision reassignment).
- `MemoryBackend` unit tests mirroring existing fs behavior.
- Two-device integration test: two `MemoryBackend` archives + one in-memory
  `RemoteStore` fake; interleave edits and `sync()` calls; assert convergence
  and no data loss. No network required.
- `SupabaseStore` request-building unit tests (URL/headers/body), network calls
  mocked.
