# Storage Backend + Remote Sync Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make storage pluggable behind a `StorageBackend` trait and add offline-first multi-device sync through a generic `RemoteStore` protocol with a Supabase implementation.

**Architecture:** All persistence already funnels through `src/storage_io.rs`; we introduce a trait there with fs/localStorage/memory implementations while keeping the existing free-function API. A new `src/sync/` module adds pure three-way merge functions, per-file sync state (`.sync/` revision map + base snapshots), an async engine generic over `RemoteStore`, and a `SupabaseStore` over PostgREST. One new async FFI function `sync()`.

**Tech Stack:** Rust 2021, serde/serde_yaml/serde_json, uniffi 0.30 (async via tokio), reqwest (rustls on native, fetch on wasm), rand 0.8 (+ getrandom "js" on wasm).

## Global Constraints

- Existing public FFI functions must keep their exact signatures and synchronous behavior.
- Everything must compile for wasm32 (`cargo check --target wasm32-unknown-unknown`) and native.
- Paths handed to `StorageBackend` are archive-relative forward-slash strings (`"goals.yaml"`, `"graphs/2026-08-09.mmd"`, `"notes/goal_1.md"`).
- `.sync/` prefix is local-only state and must never be pushed to the remote.
- Spec: `docs/superpowers/specs/2026-08-09-storage-backend-sync-design.md`.

---

### Task 1: `StorageBackend` trait, `MemoryBackend`, `FsBackend`, `LocalStorageBackend`

**Files:**
- Create: `src/backend.rs`
- Modify: `src/storage_io.rs` (delegate to backends; keep function signatures)
- Modify: `src/lib.rs` (declare `pub mod backend`)
- Test: unit tests in `src/backend.rs` + existing `tests/goals.rs` must keep passing

**Interfaces (Produces):**

```rust
pub trait StorageBackend {
    fn read(&self, path: &str) -> Result<Option<String>, StorageIoError>;
    fn write(&self, path: &str, content: &str) -> Result<(), StorageIoError>;
    fn list(&self, prefix: &str) -> Result<Vec<String>, StorageIoError>; // sorted, archive-relative
    fn delete(&self, path: &str) -> Result<(), StorageIoError>;          // ok if absent
    fn ensure_structure(&self) -> Result<(), StorageIoError>;
}
pub struct MemoryBackend { /* RefCell<BTreeMap<String,String>> */ }
pub struct FsBackend { root: PathBuf }                  // native
pub struct LocalStorageBackend { prefix: String }       // wasm32
pub fn default_backend(archive: &Path) -> impl StorageBackend; // FsBackend or LocalStorageBackend
```

- [ ] Step 1: Write failing unit tests in `src/backend.rs` for `MemoryBackend`: write/read roundtrip, read missing → `None`, `list("graphs/")` returns only matching sorted keys, `delete` then read → `None`, `delete` of missing path is `Ok`.
- [ ] Step 2: `cargo test backend` → FAIL (module missing).
- [ ] Step 3: Implement trait + `MemoryBackend` (uses `RefCell<BTreeMap>`; not `Sync`, fine for tests) + `FsBackend` (`read`/`write` mirror current fs code rooted at `root`; `list` walks `root` recursively with `std::fs::read_dir`, returns relative paths; `ensure_structure` mirrors `ensure_archive_structure`) + `LocalStorageBackend` (mirror current key scheme `"{prefix}_{path with / → __}"`; `list` iterates `storage.length()`/`storage.key(i)` and reverses the scheme).
- [ ] Step 4: Rewire `storage_io::read_to_string`/`write_string`/`ensure_archive_structure` to build `default_backend(archive)` and delegate, converting the absolute `path` arg to archive-relative via `strip_prefix`. Add `storage_io::backend_for(archive)` used by later tasks.
- [ ] Step 5: Add fs-backed tests for `FsBackend::list` with `tempfile`.
- [ ] Step 6: `cargo test` all green; `cargo check --target wasm32-unknown-unknown` green.
- [ ] Step 7: Commit `refactor: introduce StorageBackend trait with fs/localStorage/memory impls`.

### Task 2: Random goal ids

**Files:**
- Modify: `src/goals.rs` (`next_goal_id` → `new_goal_id(existing: &[Goal]) -> u64`)
- Modify: `Cargo.toml` (add `rand = "0.8"`; wasm target: `getrandom = { version = "0.2", features = ["js"] }`)
- Test: `src/goals.rs` unit test

**Interfaces (Produces):** `add_goal` behavior change only — ids are now random non-zero `u64`, unique within the archive.

- [ ] Step 1: Failing test: `new_goal_id` returns id not present in existing goals, non-zero, and two consecutive calls with same input differ (loop 10 tries to avoid flake).
- [ ] Step 2: Implement with `rand::random::<u64>()`, retry while id is 0 or already used.
- [ ] Step 3: `cargo test` green (existing tests use returned ids, so max+1 removal is safe). wasm check green.
- [ ] Step 4: Commit `feat: random goal ids to avoid multi-device collisions`.

### Task 3: Merge functions (pure)

**Files:**
- Create: `src/sync/mod.rs`, `src/sync/merge.rs`
- Modify: `src/lib.rs` (`pub mod sync` hidden from docs)
- Modify: `src/types.rs` (derive `PartialEq` on `Goal`, `Session`)
- Test: unit tests in `src/sync/merge.rs`

**Interfaces (Produces):**

```rust
pub fn merge_sessions(local: &[Session], remote: &[Session]) -> Vec<Session>;
// union keyed by (goal_id, start_at, end_at, kind); local wins on key clash;
// sorted by start_at; ids renumbered sess_N / rew_N in order.

pub struct GoalsMergeResult { pub merged: Vec<Goal>, pub reassigned: Vec<(u64, u64)> }
pub fn merge_goals(base: &[Goal], local: &[Goal], remote: &[Goal]) -> GoalsMergeResult;
// per-id three-way: one-side-only kept; equal kept; local==base → remote;
// remote==base → local; both changed → local. Same id new on BOTH sides with
// different name → keep remote under old id, local gets fresh random id
// (recorded in `reassigned`).

pub fn merge_notes(base: &str, local: &str, remote: &str) -> String;
// local==base → remote; remote==base → local; equal → local; else
// local + "\n\n---\n_Conflicting version from another device:_\n---\n\n" + remote.
```

- [ ] Step 1: Write failing tests: sessions union/dedupe/renumber; goals one-side-change, both-change-local-wins, new-on-one-side, id-collision-reassign; notes fast paths + conflict concatenation.
- [ ] Step 2: `cargo test sync::merge` → FAIL.
- [ ] Step 3: Implement; reuse `crate::goals::` random id helper for reassignment.
- [ ] Step 4: Tests green; wasm check green; commit `feat: three-way merge functions for goals, sessions, notes`.

### Task 4: Sync state + remote protocol + in-memory fake

**Files:**
- Create: `src/sync/state.rs`, `src/sync/remote.rs`
- Test: unit tests in both files

**Interfaces (Produces):**

```rust
// state.rs — persisted via StorageBackend under .sync/
pub struct SyncState { pub revisions: BTreeMap<String, i64> } // path → last synced revision
pub fn load_state(b: &dyn StorageBackend) -> Result<SyncState, AppError>;    // .sync/state.json
pub fn save_state(b: &dyn StorageBackend, s: &SyncState) -> Result<(), AppError>;
pub fn read_base(b: &dyn StorageBackend, path: &str) -> Result<Option<String>, AppError>; // .sync/base/<path>
pub fn write_base(b: &dyn StorageBackend, path: &str, content: &str) -> Result<(), AppError>;

// remote.rs
pub struct RemoteEntry { pub path: String, pub revision: i64 }
pub struct RemoteDoc { pub content: String, pub revision: i64 }
#[derive(Debug, thiserror::Error)]
pub enum SyncError { Conflict, Http { status: u16, detail: String }, Network { detail: String }, Protocol { detail: String } }
pub trait RemoteStore {
    async fn list(&self) -> Result<Vec<RemoteEntry>, SyncError>;
    async fn get(&self, path: &str) -> Result<Option<RemoteDoc>, SyncError>;
    async fn put(&self, path: &str, content: &str, base_revision: Option<i64>) -> Result<i64, SyncError>;
}
pub struct MemoryRemote { /* RefCell<BTreeMap<String, (String, i64)>> */ } // cfg(test)? no — used by integration tests, keep pub
```

- [ ] Step 1: Failing tests: state roundtrip on `MemoryBackend`; missing state → empty; `MemoryRemote` put-new (base None) → rev 1, put with stale base → `SyncError::Conflict`, get/list reflect puts. Async tests run via a tiny `pollster`-free helper: since `MemoryRemote` futures are ready immediately, use `futures::executor::block_on`-equivalent — add dev-dependency `pollster = "0.4"`.
- [ ] Step 2: Implement. `async fn` in traits (Rust ≥1.75), engine will be generic (no `dyn RemoteStore`).
- [ ] Step 3: Tests green; wasm check green; commit `feat: sync state persistence and RemoteStore protocol with in-memory fake`.

### Task 5: Sync engine + two-device integration tests

**Files:**
- Create: `src/sync/engine.rs`
- Test: `tests/sync.rs` (integration, uses `MemoryBackend` + `MemoryRemote` + `pollster`)

**Interfaces (Produces):**

```rust
pub struct SyncReport { pub pushed: u32, pub pulled: u32, pub merged: u32 }
pub async fn sync_archive<B: StorageBackend, R: RemoteStore>(backend: &B, remote: &R)
    -> Result<SyncReport, AppError>;
```

Engine algorithm:
1. `remote.list()` → `BTreeMap<path, revision>`; `backend.list("")` filtered: drop `.sync/` prefix entries.
2. Load `SyncState`. Handle `goals.yaml` FIRST: decide changed-local (`current != base`) / changed-remote (`remote rev != state rev`); on both-changed run `merge_goals`; apply each `reassigned (old,new)`: rewrite local `notes/goal_old.md` → `notes/goal_new.md` (read, write new, delete old), and in every LOCAL day file rewrite sessions with `goal_id == old` (parse via `session_graph::list_day_sessions`-equivalent on backend — add small helper reading/writing through the backend) before those files are merged.
3. For every other path in the union of local+remote sets, apply the action table (push / pull / merge). Notes merge uses `merge_notes` with base from `read_base`; day files use `merge_sessions`.
4. Push with `put(path, content, state.revisions.get(path))`; on `SyncError::Conflict` re-`get`, re-merge, retry (max 3, then error).
5. After each file's success: `write_base` + update revision in state; `save_state` once at end AND after goals.yaml (crash safety).
6. Count: push→`pushed`, pull→`pulled`, both-changed→`merged` (a merged file also pushes but counts once, as merged).

- [ ] Step 1: Failing integration tests in `tests/sync.rs`:
  - fresh device pushes all files; second empty device pulls them; archives equal.
  - device A adds goal+session, syncs; B syncs, adds session same day, syncs; A syncs → both day files contain both sessions.
  - both edit same note between syncs → conflict divider present, both texts present, archives converge after B re-syncs.
  - both add different goals offline → after cross-sync both goals on both devices.
  - `.sync/` files never appear in `MemoryRemote`.
- [ ] Step 2: Implement engine + backend-based day-file read/write helpers in `session_graph.rs` (`pub(crate) fn parse_day(content, date)` / `render_day(sessions)` extracted from existing `parse_mermaid`/`to_mermaid` — reuse, don't duplicate).
- [ ] Step 3: Tests green; wasm check green; commit `feat: offline-first sync engine with three-way merge`.

### Task 6: `SupabaseStore`

**Files:**
- Create: `src/sync/supabase.rs`, `docs/supabase-setup.sql`
- Modify: `Cargo.toml` (reqwest both targets, tokio native)
- Test: unit tests for URL/header/body construction (no network)

**Interfaces (Produces):**

```rust
pub struct SupabaseStore { base_url: String, api_key: String, archive_id: String, client: reqwest::Client }
impl SupabaseStore { pub fn new(base_url: &str, api_key: &str, archive_id: &str) -> Self }
impl RemoteStore for SupabaseStore { /* list/get/put over PostgREST /rest/v1/documents */ }
```

- `list`: `GET {base}/rest/v1/documents?archive_id=eq.{id}&select=path,revision`.
- `get`: same + `&path=eq.{path}&select=content,revision`, single row or None.
- `put` new (`base_revision: None`): `POST /rest/v1/documents` body `{archive_id, path, content, revision: 1}`; 409 → `Conflict`.
- `put` update: `PATCH ...?archive_id=eq.{id}&path=eq.{path}&revision=eq.{base}` body `{content, revision: base+1}` with `Prefer: return=representation`; empty result array → `Conflict`.
- Headers: `apikey`, `Authorization: Bearer {api_key}`, `Content-Type: application/json`.
- `docs/supabase-setup.sql`: the `create table documents (...)` from the spec.

- [ ] Step 1: Failing tests for pure helpers `list_url()`, `get_url(path)`, `patch_url(path, base)`, `insert_body(path, content)` (percent-encode `path` values).
- [ ] Step 2: Implement helpers + `RemoteStore` impl mapping reqwest errors → `SyncError::Network`, non-2xx → `Http`, decode failures → `Protocol`.
- [ ] Step 3: Tests green on native; wasm check green; commit `feat: Supabase RemoteStore implementation`.

### Task 7: FFI export + docs

**Files:**
- Modify: `src/lib.rs` (async `sync` export, `SyncReport` re-export), `src/ffi_types.rs` (`AppError::Sync { detail }`, `From<SyncError>`), `src/sync/engine.rs` (`SyncReport` as `uniffi::Record`), `Cargo.toml` (tokio), `README.md` (sync section)
- Test: `cargo test`, wasm check, uniffi scaffolding compiles

```rust
#[cfg_attr(not(target_arch = "wasm32"), uniffi::export(async_runtime = "tokio"))]
pub async fn sync(archive_path: String, remote_url: String, api_key: String, archive_id: String)
    -> Result<SyncReport, AppError> {
    let backend = storage_io::backend_for(Path::new(&archive_path));
    let remote = sync::supabase::SupabaseStore::new(&remote_url, &api_key, &archive_id);
    sync::engine::sync_archive(&backend, &remote).await
}
```

- [ ] Step 1: Add `AppError::Sync { detail: String }` + `From<SyncError>`; add export; make engine types `uniffi::Record`-compatible (u32 counters).
- [ ] Step 2: `cargo test` + `cargo clippy` + wasm check green; README section: setup SQL, config values, `sync()` usage, merge-behavior notes.
- [ ] Step 3: Commit `feat: expose async sync() over FFI with Supabase backend`.

## Verification (end-to-end)

1. `cargo test` — all unit + integration tests pass.
2. `cargo clippy --all-targets` — no warnings introduced.
3. `cargo check --target wasm32-unknown-unknown` — wasm build intact.
4. `cargo build --release && cargo run --bin uniffi-bindgen generate --library target/release/libsuccesslib.so --language kotlin --out-dir /tmp/out` — bindings generate, `sync` appears as suspend fn.
