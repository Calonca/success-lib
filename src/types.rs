//! Types used by the Success FFI surface.
//!
//! This module defines the primary domain types exposed to foreign
//! language bindings: `Goal`, `Session`, and their supporting enums.
//! These types are serializable and annotated for `uniffi` where needed.
use chrono::{Local, TimeZone, Utc};
use serde::{Deserialize, Serialize};

/// The semantic kind of a session.
///
/// - `Goal`: a session associated with a normal goal.
/// - `Reward`: a session associated with a reward goal.
#[cfg_attr(all(not(target_arch = "wasm32"), feature = "uniffi"), derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionKind {
    Goal,
    Reward,
}

/// The current status of a `Goal`.
///
/// - `TODO`: goal not yet started.
/// - `DOING`: goal in progress.
/// - `DONE`: goal completed.
#[cfg_attr(all(not(target_arch = "wasm32"), feature = "uniffi"), derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GoalStatus {
    #[default]
    TODO,
    DOING,
    DONE,
}

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

/// A goal managed in the archive.
///
/// Fields:
/// - `id`: unique numeric identifier.
/// - `name`: human-readable name.
/// - `is_reward`: whether the goal is a reward type.
/// - `commands`: optional associated commands.
/// - `status`: current `GoalStatus`.
/// - `trashed`: whether the goal is in the trash bin.
/// - `quantity_names`: names of the quantities sessions may record, empty
///   when the goal is not quantifiable.
#[cfg_attr(all(not(target_arch = "wasm32"), feature = "uniffi"), derive(uniffi::Record))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Goal {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub is_reward: bool,
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default)]
    pub status: GoalStatus,
    #[serde(default)]
    pub trashed: bool,
    #[serde(
        default,
        alias = "quantity_name",
        deserialize_with = "quantity_names_compat",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub quantity_names: Vec<String>,
}

/// A recorded session entry.
///
/// - `id`: unique string identifier for the session.
/// - `name`: human-friendly session name.
/// - `goal_id`: the associated goal's id.
/// - `kind`: whether this was a `Goal` or `Reward` session.
/// - `start_at` / `end_at`: Unix timestamps in seconds (UTC).
/// - `quantity`: optional quantity recorded during the session.
#[cfg_attr(all(not(target_arch = "wasm32"), feature = "uniffi"), derive(uniffi::Record))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub name: String,
    pub goal_id: u64,
    pub kind: SessionKind,
    #[serde(default)]
    pub quantity: Option<u32>,
    #[serde(default)]
    pub start_at: i64,
    #[serde(default)]
    pub end_at: i64,
}

/// Convert a Unix-seconds timestamp to an ISO date string (`YYYY-MM-DD`)
/// in the **local** timezone.
pub fn timestamp_to_date_iso(ts: i64) -> String {
    let dt = Utc
        .timestamp_opt(ts, 0)
        .single()
        .expect("valid timestamp");
    dt.with_timezone(&Local)
        .format("%Y-%m-%d")
        .to_string()
}

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
