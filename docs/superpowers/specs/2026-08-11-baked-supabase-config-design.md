# Design: Build-Time Supabase Config Baked Into the Library

**Date:** 2026-08-11
**Status:** Approved

## Problem

The Supabase configuration is not stored anywhere in the library. The exported
`sync()` FFI function (`src/lib.rs`) takes `remote_url`, `api_key`, and
`archive_id` as plain arguments on every call, so every consuming app must
know and supply the Supabase project URL and API key itself. The user wants
all their apps to share one database without each app duplicating that
configuration.

## Goals

1. Optionally bake the Supabase project URL and anon key into the compiled
   library at build time, sourced from an uncommitted `.env` file.
2. Apps built against a configured library can sync with only
   `(archive_path, archive_id)` — no Supabase knowledge required.
3. The explicit `sync(archive_path, remote_url, api_key, archive_id)` remains
   available as an override, unchanged.

## Non-goals

- Runtime config files or dynamic reconfiguration (rebuilding is acceptable).
- Baking `archive_id`: it identifies *whose* archive within the shared
  database, so it stays a per-call runtime parameter.
- Auth flows or per-user keys (single shared anon key, RLS protects data).

## Design

### `.env` at the crate root (uncommitted)

```
SUPABASE_URL=https://abc123.supabase.co
SUPABASE_ANON_KEY=eyJ...
```

`.env` is already in `.gitignore`. A committed `.env.example` documents the
two keys with placeholder values.

### `build.rs`

- Always emits `cargo:rerun-if-changed=.env` so edits trigger a rebuild.
- If `.env` exists, parses it with trivial hand-rolled `KEY=value` line
  parsing (no new dependency; `#` comments and blank lines skipped, no quote
  handling) and re-emits the two keys as
  `cargo:rustc-env=SUCCESS_SUPABASE_URL=...` and
  `cargo:rustc-env=SUCCESS_SUPABASE_ANON_KEY=...`.
- Missing file, or a key absent from the file → that env var is simply not
  emitted; the build proceeds normally. CI and fresh clones are unaffected.

### New FFI export: `sync_default`

In `src/lib.rs`, alongside the existing `sync()`:

```rust
#[cfg_attr(not(target_arch = "wasm32"), uniffi::export(async_runtime = "tokio"))]
pub async fn sync_default(
    archive_path: String,
    archive_id: String,
) -> Result<SyncReport, AppError>
```

- Reads the baked-in values via `option_env!("SUCCESS_SUPABASE_URL")` /
  `option_env!("SUCCESS_SUPABASE_ANON_KEY")`.
- If either is `None` (library built without a `.env`), returns
  `AppError::InvalidInput { detail: "library was built without a Supabase
  config; pass the URL and key explicitly via sync()" }`.
- Otherwise delegates to the same code path as `sync()`.

### Error handling

- Build never fails because of config: absent `.env` means absent defaults.
- The only new runtime error is the `InvalidInput` above from
  `sync_default()` on an unconfigured build.

### Testing

- `build.rs` parsing stays trivial enough not to warrant its own tests.
- New test: `sync_default()` on a normal (unconfigured) test build returns
  the `InvalidInput` error with the expected message.
- Happy-path sync behavior is already covered by existing tests through the
  explicit `sync()`; `sync_default` shares that code path.

## Security note

The anon key is embedded in every shipped app binary. That is normal for
Supabase anon keys (they are public by design), but it means row-level
security on the `documents` table is the actual protection boundary — the
RLS policies from `docs/supabase-setup.sql` must be in place on the shared
project.
