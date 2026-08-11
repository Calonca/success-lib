# Named Quantities (successlib 0.7.0) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A session can record several named quantities (`[q cards=42 known=1520]`) against a goal that declares its quantity names, replacing today's single anonymous `quantity: Option<u32>`.

**Architecture:** `Goal.quantity_name: Option<String>` becomes `quantity_names: Vec<String>` (legacy YAML still loads); `Session.quantity: Option<u32>` becomes `quantities: Vec<QuantityValue>` where `QuantityValue { name, value }`. The Mermaid day-file tag `[q N]` grows a named form `[q a=1 b=2]` (sorted by name for deterministic output); legacy bare-number tags still parse and are resolved to the goal's first declared name on read. A new default-on `uniffi` cargo feature lets Rust consumers take the crate as a plain rlib. The sync engine is untouched.

**Tech Stack:** Rust 2021, serde/serde_yaml, chrono, uniffi 0.30 (behind the new feature). Repo: `/var/home/ale/Documents/Projects/26/success-lib`.

## Global Constraints

- Work on branch `feat/backend-sync` (contains the unpushed sync engine). Do not rebase or touch the existing 9 commits.
- Version bumps to exactly `0.7.0` (Task 6).
- Quantity names must match `[a-z0-9_-]+` (non-empty, ASCII lowercase/digit/underscore/hyphen only).
- Day files must remain valid Mermaid `stateDiagram-v2`; quantity rendering must be deterministic (sorted by name).
- Existing archives (legacy `quantity_name:` YAML and `[q 5]` day files) must load without migration.
- Every task ends with `cargo test` green and a commit. Run tests from the repo root.
- The `wasm32` cfg-gating pattern already in the code is the model for the new `uniffi` feature gating — mirror it, don't invent a new style.

---

### Task 1: `uniffi` cargo feature

Make UniFFI optional so a Rust consumer can depend with `default-features = false` and get a plain rlib. Default stays on, so existing consumers and `build.sh` see no change.

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/lib.rs:34-35` (scaffolding) and every `#[cfg_attr(not(target_arch = "wasm32"), uniffi::export...)]` / `derive(uniffi::...)` site
- Modify: `src/types.rs:13,25,43,68`, `src/ffi_types.rs:3`, `src/sync/engine.rs:23`

**Interfaces:**
- Produces: cargo feature `uniffi` (default). `cargo check --no-default-features` compiles without any uniffi crate in the graph.

- [ ] **Step 1: Make the uniffi dependencies optional and declare the feature**

In `Cargo.toml`, replace the `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]` and build-deps sections and the `[[bin]]` entry:

```toml
[features]
default = ["uniffi"]
uniffi = ["dep:uniffi", "dep:uniffi_bindgen"]

[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
uniffi = { version = "0.30.0", features = ["cli", "tokio"], optional = true }
uniffi_bindgen = { version = "0.30.0", optional = true }
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }

[[bin]]
name = "uniffi-bindgen"
path = "uniffi-bindgen.rs"
required-features = ["uniffi"]
```

Delete the `[target.'cfg(not(target_arch = "wasm32"))'.build-dependencies]` section entirely — there is no `build.rs`, so it was never used.

- [ ] **Step 2: Gate every uniffi attribute on the feature**

Mechanical, repo-wide: every `not(target_arch = "wasm32")` condition that guards a uniffi item becomes `all(not(target_arch = "wasm32"), feature = "uniffi")`. Sites:

`src/lib.rs:34-35`:
```rust
#[cfg(all(not(target_arch = "wasm32"), feature = "uniffi"))]
uniffi::setup_scaffolding!();
```

Every export in `src/lib.rs` (`list_goals`, `list_trash`, `search_goals`, `add_goal`, `get_note`, `edit_note`, `set_goal_status`, `set_goal_trashed`, `add_session`, `list_day_sessions`, `sync`, `list_sessions_between_dates`):
```rust
#[cfg_attr(all(not(target_arch = "wasm32"), feature = "uniffi"), uniffi::export)]
```
(`sync` keeps its argument: `uniffi::export(async_runtime = "tokio")`.)

The derives — `src/types.rs:13,25,43,68` (`SessionKind`, `GoalStatus`, `Goal`, `Session`), `src/ffi_types.rs:3` (`AppError`), `src/sync/engine.rs:23` (`SyncReport`):
```rust
#[cfg_attr(all(not(target_arch = "wasm32"), feature = "uniffi"), derive(uniffi::Record))]
```
(keeping `uniffi::Enum` / `uniffi::Error` where that is what is derived today).

- [ ] **Step 3: Verify both configurations build and tests pass**

Run: `cargo check --no-default-features && cargo test`
Expected: both succeed. `cargo tree --no-default-features | grep -c uniffi` prints `0`.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock src/
git commit -m "feat: make uniffi optional behind a default-on feature"
```

---

### Task 2: `QuantityValue` and `Goal.quantity_names`

**Files:**
- Modify: `src/types.rs` (new type; `Goal` field)
- Modify: `src/goals.rs:69-92` (`add_goal` signature + name validation), `src/goals.rs:217-227` (test fixture)
- Modify: `src/lib.rs:91-106` (`add_goal` export — signature only, so the crate compiles; full FFI pass is Task 5)
- Modify: `tests/goals.rs` (call sites)
- Test: unit tests in `src/types.rs` and `src/goals.rs`

**Interfaces:**
- Produces: `pub struct QuantityValue { pub name: String, pub value: u32 }`; `Goal.quantity_names: Vec<String>`; `pub fn valid_quantity_name(name: &str) -> bool` in `src/types.rs`; `goals::add_goal(archive, name, is_reward, commands, quantity_names: Vec<String>)`.
- Consumes: nothing new.

- [ ] **Step 1: Write failing tests for the type change and legacy YAML**

In `src/types.rs`, append a test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_quantity_name_string_loads_as_one_element_list() {
        let yaml = "id: 5\nname: Read\nquantity_name: pages\n";
        let goal: Goal = serde_yaml::from_str(yaml).expect("legacy yaml");
        assert_eq!(goal.quantity_names, vec!["pages".to_string()]);
    }

    #[test]
    fn absent_and_null_quantity_names_load_as_empty() {
        let absent: Goal = serde_yaml::from_str("id: 5\nname: Read\n").unwrap();
        assert!(absent.quantity_names.is_empty());
        let null: Goal = serde_yaml::from_str("id: 5\nname: Read\nquantity_name: null\n").unwrap();
        assert!(null.quantity_names.is_empty());
    }

    #[test]
    fn quantity_names_round_trip_in_new_form() {
        let yaml = "id: 5\nname: Read\nquantity_names:\n- cards\n- known\n";
        let goal: Goal = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(goal.quantity_names, vec!["cards", "known"]);
        let out = serde_yaml::to_string(&goal).unwrap();
        assert!(out.contains("quantity_names"), "writes the new key: {out}");
        assert!(!out.contains("quantity_name:"), "never writes the legacy key: {out}");
    }

    #[test]
    fn quantity_name_charset_is_enforced() {
        for good in ["cards", "known", "max-known", "q_2"] {
            assert!(valid_quantity_name(good), "{good} should be valid");
        }
        for bad in ["", "Cards", "max known", "a=b", "q:1", "漢字"] {
            assert!(!valid_quantity_name(bad), "{bad:?} should be invalid");
        }
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test --lib types`
Expected: FAIL — `quantity_names`, `valid_quantity_name` do not exist.

- [ ] **Step 3: Implement the type change**

In `src/types.rs`, add above `Goal`:

```rust
/// One named measurement recorded during a session, e.g. `cards=42`.
///
/// The name must satisfy [`valid_quantity_name`] — lowercase ASCII, digits,
/// `_`, `-` — so the Mermaid tag `[q name=value ...]` can never contain a
/// character that breaks the diagram or the parser.
#[cfg_attr(all(not(target_arch = "wasm32"), feature = "uniffi"), derive(uniffi::Record))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuantityValue {
    pub name: String,
    pub value: u32,
}

/// Whether `name` may be used as a quantity name.
pub fn valid_quantity_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// Accepts the legacy scalar form (`quantity_name: pages`) as well as the
/// current list form, so archives written before 0.7.0 load unchanged.
fn quantity_names_compat<'de, D>(de: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Compat {
        Many(Vec<String>),
        One(String),
    }
    Ok(match Option::<Compat>::deserialize(de)? {
        None => vec![],
        Some(Compat::One(name)) => vec![name],
        Some(Compat::Many(names)) => names,
    })
}
```

Replace the `Goal.quantity_name` field:

```rust
    #[serde(
        default,
        alias = "quantity_name",
        deserialize_with = "quantity_names_compat",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub quantity_names: Vec<String>,
```

Update the `Goal` doc comment's field list accordingly (`quantity_names`: names of the quantities sessions may record, empty when the goal is not quantifiable).

- [ ] **Step 4: Update `goals.rs` to the new field and validate names**

`src/goals.rs:69-92` — new signature and validation:

```rust
pub fn add_goal(
    archive: &Path,
    name: &str,
    is_reward: bool,
    commands: Vec<String>,
    quantity_names: Vec<String>,
) -> Result<Goal, AppError> {
    for q in &quantity_names {
        if !crate::types::valid_quantity_name(q) {
            return Err(AppError::InvalidInput {
                detail: format!(
                    "invalid quantity name {q:?}: use lowercase ASCII, digits, '_' or '-'"
                ),
            });
        }
    }
    let mut goals = read_goals(archive)?;
    let id = new_goal_id(&goals);
    let goal = Goal {
        id,
        name: name.to_string(),
        is_reward,
        commands,
        status: GoalStatus::TODO,
        trashed: false,
        quantity_names,
    };
    goals.push(goal.clone());

    write_goals(archive, &goals)?;

    Ok(goal)
}
```

Also add (below `get_goal`) a reader that does not filter, for the legacy-tag resolution in Task 4:

```rust
/// Every goal in the archive, trashed and DONE included. The legacy `[q N]`
/// resolution needs the full list: a session may reference a goal that has
/// since been finished or trashed.
pub fn all_goals(archive: &Path) -> Result<Vec<Goal>, AppError> {
    read_goals(archive)
}
```

Fix the test fixture at `src/goals.rs:217-227`: `quantity_name: None` → `quantity_names: vec![]`.

Add a validation test to the `tests` module in `src/goals.rs`:

```rust
    #[test]
    fn add_goal_rejects_bad_quantity_names() {
        let dir = tempfile::tempdir().unwrap();
        let err = add_goal(dir.path(), "g", false, vec![], vec!["Bad Name".into()]);
        assert!(matches!(err, Err(AppError::InvalidInput { .. })));
        let ok = add_goal(dir.path(), "g", false, vec![], vec!["cards".into(), "known".into()]);
        assert_eq!(ok.unwrap().quantity_names, vec!["cards", "known"]);
    }
```

- [ ] **Step 5: Mend the remaining compile errors minimally**

`src/lib.rs:92-106` (`add_goal` export): parameter becomes `quantity_names: Vec<String>` and is passed through. `src/lib.rs:32`: add `QuantityValue` and `valid_quantity_name` to the `pub use types::{...}` re-export — later tasks and external consumers use `successlib::QuantityValue`. `tests/goals.rs`: update every `add_goal(...)` call — `None` → `vec![]`, `Some("x".into())` → `vec!["x".into()]`. Any other `quantity_name:` struct literal in the tree (`src/sync/merge.rs` tests) — `grep -rn "quantity_name" src/ tests/` — becomes `quantity_names: vec![]` (or the list form). Do NOT touch `Session.quantity` yet; that is Task 3.

- [ ] **Step 6: Run the full suite**

Run: `cargo test && cargo check --no-default-features`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/ tests/
git commit -m "feat: goals declare named quantities, legacy quantity_name still loads"
```

---

### Task 3: `Session.quantities` — Mermaid render and parse

**Files:**
- Modify: `src/types.rs:60-81` (`Session` field + doc)
- Modify: `src/session_graph.rs` — `to_mermaid` (line ~322), `split_label` (line ~233), `add_session` construction (~line 62), plus its unit tests
- Modify: `src/sync/merge.rs` test fixtures (~lines 173-224), `tests/sync.rs` fixtures
- Test: unit tests in `src/session_graph.rs`

**Interfaces:**
- Consumes: `QuantityValue` from Task 2.
- Produces: `Session.quantities: Vec<QuantityValue>`. On-disk named tag `[q a=1 b=2]` (sorted by name); legacy `[q 5]` parses to `vec![QuantityValue { name: "".into(), value: 5 }]` and renders back unchanged (the empty name is the "legacy, not yet resolved" sentinel — it round-trips so sync merges never rewrite a file they didn't change).

- [ ] **Step 1: Write failing round-trip tests**

In `src/session_graph.rs`'s existing `#[cfg(test)] mod tests` (create one if absent), add:

```rust
    use crate::types::QuantityValue;

    fn qv(name: &str, value: u32) -> QuantityValue {
        QuantityValue { name: name.into(), value }
    }

    #[test]
    fn named_quantities_render_sorted_and_round_trip() {
        let date = chrono::NaiveDate::from_ymd_opt(2026, 8, 11).unwrap();
        let start = chrono::Local
            .with_ymd_and_hms(2026, 8, 11, 9, 0, 0)
            .unwrap()
            .with_timezone(&chrono::Utc);
        let session = Session {
            id: "sess_1".into(),
            name: "Japanese".into(),
            goal_id: 123,
            kind: SessionKind::Goal,
            quantities: vec![qv("known", 1520), qv("cards", 42)],
            start_at: start.timestamp(),
            end_at: start.timestamp() + 1500,
        };
        let text = to_mermaid(&[session.clone()]);
        assert!(
            text.contains("[q cards=42 known=1520]"),
            "sorted by name regardless of input order: {text}"
        );
        let parsed = parse_mermaid(&text, date).unwrap();
        assert_eq!(parsed, vec![session]);
    }

    #[test]
    fn legacy_bare_quantity_parses_and_round_trips_unchanged() {
        let date = chrono::NaiveDate::from_ymd_opt(2026, 8, 11).unwrap();
        let content = "stateDiagram-v2\n    [*] --> sess_1\n    sess_1: Read [id 7] [q 5] [09#colon;00-10#colon;00]\n";
        let parsed = parse_mermaid(content, date).unwrap();
        assert_eq!(parsed[0].quantities, vec![qv("", 5)]);
        let rendered = to_mermaid(&parsed);
        assert!(rendered.contains("[q 5]"), "legacy form is preserved: {rendered}");
        assert!(!rendered.contains('='), "no named form invented: {rendered}");
    }

    #[test]
    fn a_session_without_quantities_has_no_q_tag() {
        let date = chrono::NaiveDate::from_ymd_opt(2026, 8, 11).unwrap();
        let content = "stateDiagram-v2\n    [*] --> sess_1\n    sess_1: Read [id 7] [09#colon;00-10#colon;00]\n";
        let parsed = parse_mermaid(content, date).unwrap();
        assert!(parsed[0].quantities.is_empty());
        assert!(!to_mermaid(&parsed).contains("[q"));
    }

    #[test]
    fn a_malformed_q_tag_is_left_in_the_name_rather_than_guessed_at() {
        let date = chrono::NaiveDate::from_ymd_opt(2026, 8, 11).unwrap();
        let content = "stateDiagram-v2\n    [*] --> sess_1\n    sess_1: Read [id 7] [q cards=x] [09#colon;00-10#colon;00]\n";
        let parsed = parse_mermaid(content, date).unwrap();
        assert!(parsed[0].quantities.is_empty());
        assert!(parsed[0].name.contains("[q cards=x]"), "kept as text: {}", parsed[0].name);
    }
```

(`with_ymd_and_hms` needs `chrono::TimeZone` in scope — the module already imports it.)

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test --lib session_graph`
Expected: FAIL — `quantities` does not exist on `Session`.

- [ ] **Step 3: Change the `Session` type**

`src/types.rs:70-81`:

```rust
pub struct Session {
    pub id: String,
    pub name: String,
    pub goal_id: u64,
    pub kind: SessionKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quantities: Vec<QuantityValue>,
    #[serde(default)]
    pub start_at: i64,
    #[serde(default)]
    pub end_at: i64,
}
```

Update the doc comment: `quantities`: named measurements recorded during the session; an entry with an empty name is a legacy single quantity not yet resolved against its goal.

- [ ] **Step 4: Implement rendering in `to_mermaid`**

Replace the `qty` construction at `src/session_graph.rs:329-332`:

```rust
        let qty = match n.quantities.as_slice() {
            [] => String::new(),
            [q] if q.name.is_empty() => format!(" [q {}]", q.value),
            qs => {
                let mut sorted: Vec<&QuantityValue> = qs.iter().collect();
                sorted.sort_by(|a, b| a.name.cmp(&b.name));
                let body: Vec<String> =
                    sorted.iter().map(|q| format!("{}={}", q.name, q.value)).collect();
                format!(" [q {}]", body.join(" "))
            }
        };
```

Add `QuantityValue` to the module's `use crate::types::{...}` import.

- [ ] **Step 5: Implement parsing in `split_label`**

Change the function's return type (`Option<u32>` → `Vec<QuantityValue>`, both in the signature at line ~233-240 and the local at ~251) and replace the `[q ...]` branch at lines 270-280:

```rust
        if let Some(q_tail) = tag.strip_prefix('q') {
            let body = q_tail
                .trim()
                .trim_start_matches(|c: char| c == ':' || c.is_whitespace());
            if body.contains('=') {
                let mut parsed = Vec::new();
                let ok = body.split_whitespace().all(|pair| {
                    match pair.split_once('=') {
                        Some((qname, value)) if !qname.is_empty() => match value.parse::<u32>() {
                            Ok(v) => {
                                parsed.push(QuantityValue { name: qname.to_string(), value: v });
                                true
                            }
                            Err(_) => false,
                        },
                        _ => false,
                    }
                });
                if ok && !parsed.is_empty() {
                    quantities = parsed;
                    name = head.trim().to_string();
                    continue;
                }
            } else if let Ok(v) = body.parse::<u32>() {
                quantities = vec![QuantityValue { name: String::new(), value: v }];
                name = head.trim().to_string();
                continue;
            }
        }
```

(`let mut quantity = None;` at line 251 becomes `let mut quantities = Vec::new();`.) Update the one construction site in `parse_mermaid` (~line 216-224): `quantity,` → `quantities: quantities.clone(),` — or restructure so the tuple moves; the tuple from `split_label` is `(name, goal_id, quantities, explicit_time)`.

- [ ] **Step 6: Mend construction sites**

`add_session` (line ~34-81): parameter `quantity: Option<u32>` becomes `quantities: Vec<QuantityValue>`; the node construction uses `quantities` (validation is Task 4 — for now keep the existing "is the goal quantifiable" check as: if `!quantities.is_empty()` and `goal.quantity_names.is_empty()`, return the existing `InvalidInput`). `src/lib.rs` `add_session` export: parameter `quantities: Vec<QuantityValue>`, passed through (import `types::QuantityValue`). `src/sync/merge.rs` test fixtures: `quantity: None` → `quantities: vec![]`; the `local.quantity = Some(5)` / `remote.quantity = Some(9)` / assertion lines (~219-224) become:

```rust
        local.quantities = vec![QuantityValue { name: String::new(), value: 5 }];
        remote.quantities = vec![QuantityValue { name: String::new(), value: 9 }];
        // ...
        assert_eq!(merged[0].quantities, vec![QuantityValue { name: String::new(), value: 5 }]);
```

`tests/sync.rs` and `tests/goals.rs`: update every `Session { .. quantity: .. }` literal and `add_session(...)` call the same way (`None` → `vec![]`, `Some(5)` → `vec![QuantityValue { name: "".into(), value: 5 }]` where the goal predates names, or a named vec where the test created a named goal).

- [ ] **Step 7: Run the full suite**

Run: `cargo test && cargo check --no-default-features`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src/ tests/
git commit -m "feat: sessions carry named quantities in the day-file q tag"
```

---

### Task 4: `add_session` validation and legacy resolution on read

**Files:**
- Modify: `src/session_graph.rs` — `add_session` (validation), `list_day_sessions` (resolution)
- Test: unit tests in `src/session_graph.rs`

**Interfaces:**
- Consumes: `goals::get_goal`, `goals::all_goals` (Task 2), `Goal.quantity_names`.
- Produces: `add_session` rejects undeclared or duplicate quantity names; `list_day_sessions` returns legacy quantities resolved to the goal's first declared name (files on disk are not rewritten by reads).

- [ ] **Step 1: Write failing tests**

In `src/session_graph.rs` tests (using `tempfile::tempdir`, and `crate::goals::add_goal`):

```rust
    fn start_at(h: u32, m: u32) -> chrono::DateTime<chrono::Utc> {
        chrono::Local
            .with_ymd_and_hms(2026, 8, 11, h, m, 0)
            .unwrap()
            .with_timezone(&chrono::Utc)
    }

    #[test]
    fn an_undeclared_quantity_name_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let goal = crate::goals::add_goal(dir.path(), "J", false, vec![], vec!["cards".into()]).unwrap();
        let err = add_session(dir.path(), goal.id, "J", start_at(9, 0), 60, false,
            vec![qv("pages", 3)]);
        assert!(matches!(err, Err(AppError::InvalidInput { .. })));
    }

    #[test]
    fn a_duplicate_quantity_name_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let goal = crate::goals::add_goal(dir.path(), "J", false, vec![], vec!["cards".into()]).unwrap();
        let err = add_session(dir.path(), goal.id, "J", start_at(9, 0), 60, false,
            vec![qv("cards", 1), qv("cards", 2)]);
        assert!(matches!(err, Err(AppError::InvalidInput { .. })));
    }

    #[test]
    fn a_subset_of_declared_quantities_is_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let goal = crate::goals::add_goal(dir.path(), "J", false, vec![],
            vec!["cards".into(), "known".into()]).unwrap();
        let s = add_session(dir.path(), goal.id, "J", start_at(9, 0), 60, false,
            vec![qv("cards", 42)]).unwrap();
        assert_eq!(s.quantities, vec![qv("cards", 42)]);
    }

    #[test]
    fn reading_a_legacy_day_file_resolves_the_goals_first_quantity_name() {
        let dir = tempfile::tempdir().unwrap();
        let goal = crate::goals::add_goal(dir.path(), "Read", false, vec![], vec!["pages".into()]).unwrap();
        let date = chrono::NaiveDate::from_ymd_opt(2026, 8, 11).unwrap();
        let content = format!(
            "stateDiagram-v2\n    [*] --> sess_1\n    sess_1: Read [id {}] [q 5] [09#colon;00-10#colon;00]\n",
            goal.id
        );
        let path = dir.path().join("graphs").join("2026-08-11.mmd");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &content).unwrap();
        let sessions = list_day_sessions(dir.path(), date).unwrap();
        assert_eq!(sessions[0].quantities, vec![qv("pages", 5)]);
        // Reading must not rewrite the file.
        assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
    }
```

- [ ] **Step 2: Run to verify the new behaviours fail**

Run: `cargo test --lib session_graph`
Expected: the four new tests FAIL (undeclared name currently accepted when the goal has any names; legacy read returns the empty-name sentinel).

- [ ] **Step 3: Implement validation in `add_session`**

Replace the check at the top of `add_session` (the Task 3 interim version):

```rust
    if !quantities.is_empty() {
        let goal = get_goal(archive, goal_id)?;
        let mut seen = std::collections::HashSet::new();
        for q in &quantities {
            if !goal.quantity_names.contains(&q.name) {
                return Err(AppError::InvalidInput {
                    detail: format!("Goal {goal_id} has no quantity named {:?}", q.name),
                });
            }
            if !seen.insert(q.name.as_str()) {
                return Err(AppError::InvalidInput {
                    detail: format!("duplicate quantity {:?}", q.name),
                });
            }
        }
    }
```

(Empty-name sentinels can never pass this — `valid_quantity_name` forbids `""` at `add_goal`, so `quantity_names` never contains it. That is intended: new writes must use real names.)

- [ ] **Step 4: Implement resolution in `list_day_sessions`**

```rust
pub fn list_day_sessions(archive: &Path, date: NaiveDate) -> Result<Vec<Session>, AppError> {
    ensure_archive_structure(archive)?;
    let mermaid_path = day_mermaid_path(archive, date);
    if let Some(content) = storage_io::read_to_string(archive, &mermaid_path)? {
        let mut sessions = parse_mermaid(&content, date)?;
        resolve_legacy_quantities(archive, &mut sessions);
        return Ok(sessions);
    }
    Ok(vec![])
}

/// Give a legacy bare `[q N]` its meaning: the first quantity name its goal
/// declares. Goals are loaded at most once, and only when a legacy entry is
/// present; a session whose goal is gone (or declares no names) keeps the
/// empty-name sentinel rather than guessing.
fn resolve_legacy_quantities(archive: &Path, sessions: &mut [Session]) {
    let mut first_names: Option<HashMap<u64, String>> = None;
    for s in sessions.iter_mut() {
        let legacy = matches!(s.quantities.as_slice(), [q] if q.name.is_empty());
        if !legacy {
            continue;
        }
        let map = first_names.get_or_insert_with(|| {
            crate::goals::all_goals(archive)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|g| g.quantity_names.first().cloned().map(|n| (g.id, n)))
                .collect()
        });
        if let Some(n) = map.get(&s.goal_id) {
            s.quantities[0].name = n.clone();
        }
    }
}
```

(`HashMap` is already imported in this module.) Note `add_session` re-saves the day through this resolved list, so adding a session to a day with legacy entries upgrades them to named form on disk — deterministic and still parseable, and sync merges it like any other local edit.

- [ ] **Step 5: Run the full suite**

Run: `cargo test`
Expected: PASS. Sync tests must be green untouched by this step — they exercise `parse_mermaid`/`to_mermaid` directly, which round-trip the sentinel without resolution.

- [ ] **Step 6: Commit**

```bash
git add src/
git commit -m "feat: validate session quantities against the goal and resolve legacy tags on read"
```

---

### Task 5: Mermaid-validity contract test

The day files are read by humans through Mermaid renderers; lock the emitted format down so no future quantity change can silently break the diagram.

**Files:**
- Test: `tests/mermaid_format.rs` (create)

**Interfaces:**
- Consumes: `successlib::{add_goal, add_session}` public API, `successlib::session_graph` internals not needed — drive through the archive.

- [ ] **Step 1: Write the contract test**

Create `tests/mermaid_format.rs`:

```rust
//! The day files double as Mermaid stateDiagram-v2 documents. This locks the
//! emitted line grammar: `id: name [id N] [q ...]? [HH#colon;MM-HH#colon;MM]`,
//! with `:` never appearing raw after the first separator (Mermaid treats it
//! as a new description) and the q-tag body restricted to the name charset,
//! digits, `=` and single spaces.

use successlib::{add_goal, add_session, QuantityValue};

fn qv(name: &str, value: u32) -> QuantityValue {
    QuantityValue { name: name.into(), value }
}

#[test]
fn emitted_day_files_are_valid_state_diagrams() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().to_str().unwrap().to_string();
    let goal = add_goal(archive.clone(), "Japanese".into(), false, vec![],
        vec!["cards".into(), "known".into()]).unwrap();
    let session = add_session(archive.clone(), goal.id, "Japanese".into(),
        1_786_766_400, 1500, false, vec![qv("known", 1520), qv("cards", 42)]).unwrap();

    let date = successlib::timestamp_to_date_iso(session.start_at);
    let path = dir.path().join("graphs").join(format!("{date}.mmd"));
    let text = std::fs::read_to_string(path).unwrap();

    let mut lines = text.lines();
    assert_eq!(lines.next(), Some("stateDiagram-v2"));
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.contains("-->") {
            continue;
        }
        // A session line: exactly one raw ':' — the id/description separator.
        assert_eq!(
            trimmed.matches(':').count(),
            1,
            "raw ':' beyond the separator breaks Mermaid: {trimmed}"
        );
        // The q tag, when present, holds only name=value pairs.
        if let Some(q_start) = trimmed.find("[q ") {
            let body_start = q_start + 3;
            let body_end = body_start + trimmed[body_start..].find(']').expect("closed tag");
            let body = &trimmed[body_start..body_end];
            assert!(
                body.chars().all(|c| c.is_ascii_lowercase()
                    || c.is_ascii_digit()
                    || matches!(c, '_' | '-' | '=' | ' ')),
                "q tag smuggled a hostile character: {body:?}"
            );
            assert_eq!(body, "cards=42 known=1520", "sorted, space-separated: {body:?}");
        }
    }
}
```

- [ ] **Step 2: Run it**

Run: `cargo test --test mermaid_format`
Expected: PASS (Tasks 3-4 already implemented the behaviour; this is a contract, not new code). If it fails, the implementation — not the test — is wrong.

- [ ] **Step 3: Commit**

```bash
git add tests/mermaid_format.rs
git commit -m "test: lock the day-file grammar to valid Mermaid"
```

---

### Task 6: FFI surface polish, `SyncReport` serde, version 0.7.0, README

**Files:**
- Modify: `src/lib.rs` (docs, `QuantityValue` re-export), `src/sync/engine.rs:23` (serde derives), `Cargo.toml` (version), `README.md` (API table)

**Interfaces:**
- Produces: `successlib::QuantityValue` re-export; `SyncReport: Serialize + Deserialize`; crate version `0.7.0`. This is the API surface the jpdbexperiments plan builds against:
  `add_goal(archive_path: String, name: String, is_reward: bool, commands: Vec<String>, quantity_names: Vec<String>) -> Result<Goal, Error>`;
  `add_session(archive_path: String, goal_id: u64, goal_name: String, start_ts_secs: i64, duration_secs: u32, is_reward: bool, quantities: Vec<QuantityValue>) -> Result<Session, Error>`;
  `list_goals(archive_path: String, statuses: Option<Vec<GoalStatus>>) -> Result<Vec<Goal>, Error>`;
  `async sync(archive_path: String, remote_url: String, api_key: String, archive_id: String) -> Result<SyncReport, Error>`.

- [ ] **Step 1: Write the failing serde test**

In `src/sync/engine.rs` tests (or a new `#[cfg(test)]` block near `SyncReport`):

```rust
    #[test]
    fn sync_report_serialises_for_json_consumers() {
        let report = SyncReport { pushed: 1, pulled: 2, merged: 3 };
        let json = serde_json::to_string(&report).unwrap();
        assert_eq!(json, r#"{"pushed":1,"pulled":2,"merged":3}"#);
    }
```

(Adjust the field list to `SyncReport`'s actual fields at `src/sync/engine.rs:23` if it carries more — the test must name them all.)

- [ ] **Step 2: Run to verify it fails, then implement**

Run: `cargo test --lib sync` — FAIL (no `Serialize`). Extend `SyncReport`'s existing derive at `src/sync/engine.rs:24` — keep `Debug, Default, Clone` and append:

```rust
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
```

(the feature-gated `uniffi::Record` cfg_attr line above it stays as is).

- [ ] **Step 3: Re-export, document, bump**

`src/lib.rs`: confirm `QuantityValue` is in the `pub use types::{...}` list (added in Task 2). Update the doc comments on `add_goal` (quantity_names + charset rule) and `add_session` (quantities must be declared on the goal; subset allowed). `Cargo.toml`: `version = "0.7.0"`. `README.md`: update the `add_goal`/`add_session` rows of the API table to the new signatures and add one sentence under sessions: sessions record named quantities, e.g. `cards=42 known=1520`; old single-quantity archives load unchanged.

- [ ] **Step 4: Full verification**

Run: `cargo test && cargo check --no-default-features && cargo build`
Expected: all green.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: export QuantityValue, serde SyncReport, bump to 0.7.0"
```
