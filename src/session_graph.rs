use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use chrono::{
    DateTime, Duration as ChronoDuration, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc,
};

use crate::ffi_types::AppError;
use crate::goals::{get_goal, set_goal_status};
use crate::storage_io;
use crate::types::{GoalStatus, QuantityValue, Session, SessionKind};

pub fn ensure_archive_structure(archive: &Path) -> Result<(), AppError> {
    storage_io::ensure_archive_structure(archive)?;
    Ok(())
}

pub fn get_formatted_session_time_range(node: &Session) -> String {
    let start = Utc
        .timestamp_opt(node.start_at, 0)
        .single()
        .unwrap()
        .with_timezone(&Local)
        .format("%H:%M");
    let end = Utc
        .timestamp_opt(node.end_at, 0)
        .single()
        .unwrap()
        .with_timezone(&Local)
        .format("%H:%M");
    format!("{start}-{end}")
}

pub fn add_session(
    archive: &Path,
    goal_id: u64,
    goal_name: &str,
    start_at: DateTime<Utc>,
    duration_secs: u32,
    is_reward: bool,
    quantities: Vec<QuantityValue>,
) -> Result<Session, AppError> {
    ensure_archive_structure(archive)?;
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
    let day = start_at.with_timezone(&Local).date_naive();
    let mut nodes = list_day_sessions(archive, day).unwrap_or_default();
    let kind = if is_reward {
        SessionKind::Reward
    } else {
        SessionKind::Goal
    };
    let id = next_session_id(&nodes, kind);
    let end_at = start_at + ChronoDuration::seconds(duration_secs as i64);

    let node = Session {
        id,
        name: goal_name.to_string(),
        goal_id,
        kind,
        quantities,
        start_at: start_at.timestamp(),
        end_at: end_at.timestamp(),
    };

    if !is_reward {
        if let Err(err) = set_goal_status(archive, goal_id, GoalStatus::DOING) {
            eprintln!("Failed to update goal status to DOING: {err}");
        }
    }

    nodes.push(node.clone());
    save_day_sessions(archive, &nodes, day)?;
    Ok(node)
}

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

pub fn list_sessions_between_dates(
    archive: &Path,
    start_date_iso: Option<&str>,
    end_date_iso: Option<&str>,
) -> Result<Vec<Session>, AppError> {
    let end_date = if let Some(iso) = end_date_iso {
        NaiveDate::parse_from_str(iso, "%Y-%m-%d").map_err(|e| AppError::InvalidInput {
            detail: format!("invalid end date: {e}"),
        })?
    } else {
        Local::now().date_naive()
    };

    let start_date = if let Some(iso) = start_date_iso {
        NaiveDate::parse_from_str(iso, "%Y-%m-%d").map_err(|e| AppError::InvalidInput {
            detail: format!("invalid start date: {e}"),
        })?
    } else {
        end_date - ChronoDuration::days(7)
    };

    let mut sessions = Vec::new();
    let mut current = start_date;
    while current <= end_date {
        let day_sessions = list_day_sessions(archive, current).unwrap_or_default();
        sessions.extend(day_sessions);
        current += ChronoDuration::days(1);
    }

    sessions.sort_by_key(|s| s.start_at);

    Ok(sessions)
}

pub fn save_day_sessions(
    archive: &Path,
    nodes: &[Session],
    date: NaiveDate,
) -> Result<(), AppError> {
    let mut sorted = nodes.to_vec();
    sorted.sort_by_key(|n| n.start_at);

    let mermaid_path = day_mermaid_path(archive, date);
    let mermaid = to_mermaid(&sorted);
    storage_io::write_string(archive, &mermaid_path, &mermaid)?;

    Ok(())
}

fn day_key(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

fn day_mermaid_path(archive: &Path, date: NaiveDate) -> PathBuf {
    archive
        .join("graphs")
        .join(format!("{}.mmd", day_key(date)))
}

fn next_session_id(nodes: &[Session], kind: SessionKind) -> String {
    let counter = nodes.iter().filter(|n| n.kind == kind).count() + 1;
    match kind {
        SessionKind::Goal => format!("sess_{counter}"),
        SessionKind::Reward => format!("rew_{counter}"),
    }
}

/// Parse a day file's mermaid content into sessions (used by storage and sync).
pub(crate) fn parse_mermaid(content: &str, date: NaiveDate) -> Result<Vec<Session>, AppError> {
    let mut nodes = Vec::new();
    let mut labels = HashMap::new();
    let mut edges: Vec<(String, String)> = Vec::new();
    let mut start_from_entry: Option<String> = None;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.contains(':') {
            let parts: Vec<_> = trimmed.splitn(2, ':').collect();
            if parts.len() == 2 {
                let id = sanitize_id(parts[0].trim());
                let label = parts[1].trim().to_string();
                labels.insert(id, label);
            }
        }
        if trimmed.contains("-->") {
            let parts: Vec<_> = trimmed.split("-->").collect();
            if parts.len() == 2 {
                let a_raw = parts[0].trim();
                let b_raw = parts[1].trim();
                if a_raw == "[*]" {
                    start_from_entry = Some(sanitize_id(b_raw));
                } else {
                    edges.push((sanitize_id(a_raw), sanitize_id(b_raw)));
                }
            }
        }
    }
    let mut incoming = HashSet::new();
    let mut outgoing = HashMap::new();
    for (a, b) in &edges {
        incoming.insert(b.clone());
        outgoing.insert(a.clone(), b.clone());
    }
    let start = start_from_entry
        .or_else(|| {
            edges
                .iter()
                .map(|(a, _)| a)
                .find(|a| !incoming.contains(*a))
                .cloned()
        })
        .or_else(|| labels.keys().next().cloned());

    let mut cursor = start;
    while let Some(id) = cursor {
        if let Some(label) = labels.get(&id) {
            let (name, goal_id, quantities, explicit_time) = split_label(label, date);
            let clean_id = sanitize_id(&id);
            let kind = if clean_id.starts_with("rew_") {
                SessionKind::Reward
            } else {
                SessionKind::Goal
            };

            if let Some((start_at, end_at)) = explicit_time {
                nodes.push(Session {
                    id: clean_id,
                    name,
                    goal_id,
                    kind,
                    quantities,
                    start_at: start_at.timestamp(),
                    end_at: end_at.timestamp(),
                });
            }
        }
        cursor = outgoing.get(&id).cloned();
    }
    Ok(nodes)
}

#[allow(clippy::type_complexity)]
fn split_label(
    label: &str,
    date: NaiveDate,
) -> (
    String,
    u64,
    Vec<QuantityValue>,
    Option<(DateTime<Utc>, DateTime<Utc>)>,
) {
    let (without_time, time_range) = match label.rsplit_once('[') {
        Some((head, tail)) => (
            head.trim(),
            parse_time_range(tail.trim_end_matches(']').trim(), date),
        ),
        None => (label.trim(), None),
    };

    let mut goal_id = 0;
    let mut quantities = Vec::new();
    let mut name = without_time.trim().to_string();

    loop {
        let Some((head, tail)) = name.rsplit_once('[') else {
            break;
        };
        let tag = tail.trim_end_matches(']').trim();
        if let Some(id_tail) = tag.strip_prefix("id") {
            if let Ok(id_val) = id_tail
                .trim()
                .trim_start_matches(|c: char| c == ':' || c.is_whitespace())
                .parse::<u64>()
            {
                goal_id = id_val;
                name = head.trim().to_string();
                continue;
            }
        }
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
        break;
    }

    (name, goal_id, quantities, time_range)
}

fn parse_time_range(range: &str, date: NaiveDate) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let normalized = range.replace("#colon;", ":");
    let (start_raw, end_raw) = normalized.split_once('-')?;
    let start_time = NaiveTime::parse_from_str(start_raw.trim(), "%H:%M").ok()?;
    let end_time = NaiveTime::parse_from_str(end_raw.trim(), "%H:%M").ok()?;

    let start_naive = NaiveDateTime::new(date, start_time);
    let mut end_naive = NaiveDateTime::new(date, end_time);
    if end_naive < start_naive {
        end_naive += ChronoDuration::days(1);
    }

    let start_local = local_from_naive(start_naive);
    let end_local = local_from_naive(end_naive);
    Some((
        start_local.with_timezone(&Utc),
        end_local.with_timezone(&Utc),
    ))
}

fn local_from_naive(dt: NaiveDateTime) -> DateTime<Local> {
    Local.from_local_datetime(&dt).single().unwrap_or_else(|| {
        Local
            .timestamp_opt(dt.and_utc().timestamp(), 0)
            .single()
            .unwrap()
    })
}

fn sanitize_id(id: &str) -> String {
    id.replace('-', "_")
}

/// Render sessions (already sorted by start time) as a day file's mermaid
/// content (used by storage and sync).
pub(crate) fn to_mermaid(nodes: &[Session]) -> String {
    let mut out = String::from("stateDiagram-v2\n");
    if let Some(first) = nodes.first() {
        out.push_str(&format!("    [*] --> {}\n", first.id));
    }
    for (i, n) in nodes.iter().enumerate() {
        let times = format_time_range_for_mermaid(n);
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
        out.push_str(&format!(
            "    {}: {} [id {}]{} [{}]\n",
            n.id, n.name, n.goal_id, qty, times
        ));
        if let Some(next) = nodes.get(i + 1) {
            out.push_str(&format!("    {} --> {}\n", n.id, next.id));
        }
    }
    out
}

fn format_time_range_for_mermaid(node: &Session) -> String {
    fn hhmm_encoded(ts: i64) -> String {
        Utc.timestamp_opt(ts, 0)
            .single()
            .unwrap()
            .with_timezone(&Local)
            .format("%H:%M")
            .to_string()
            .replace(':', "#colon;")
    }

    let start = hhmm_encoded(node.start_at);
    let end = hhmm_encoded(node.end_at);
    format!("{start}-{end}")
}

#[cfg(test)]
mod tests {
    use super::*;
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
        // Parsing the sorted rendering yields the canonical (sorted) order.
        let mut expected = session;
        expected.quantities.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(parsed, vec![expected]);
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
}
