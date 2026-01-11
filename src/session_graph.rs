use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::{
    DateTime, Duration as ChronoDuration, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc,
};

use crate::goals::set_goal_status;
use crate::types::{GoalStatus, Session, SessionKind};

pub fn ensure_archive_structure(archive: &Path) -> Result<()> {
    fs::create_dir_all(archive)?;
    fs::create_dir_all(archive.join("graphs"))?;
    fs::create_dir_all(archive.join("notes"))?;
    if !archive.join("goals.yaml").exists() {
        fs::write(archive.join("goals.yaml"), "[]")?;
    }
    Ok(())
}

pub fn get_formatted_session_time_range(node: &Session) -> String {
    let start = node.start_at.with_timezone(&Local).format("%H:%M");
    let end = node.end_at.with_timezone(&Local).format("%H:%M");
    format!("{start}-{end}")
}

pub fn add_session(
    archive: &Path,
    goal_id: u64,
    goal_name: &str,
    start_at: DateTime<Utc>,
    duration_secs: u32,
    is_reward: bool,
) -> Result<Session> {
    ensure_archive_structure(archive)?;
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
        start_at,
        end_at,
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

pub fn list_day_sessions(archive: &Path, date: NaiveDate) -> Result<Vec<Session>> {
    ensure_archive_structure(archive)?;
    let mermaid_path = day_mermaid_path(archive, date);
    if mermaid_path.exists() {
        let content = fs::read_to_string(&mermaid_path)?;
        return parse_mermaid(&content, date);
    }
    Ok(vec![])
}

pub fn save_day_sessions(archive: &Path, nodes: &[Session], date: NaiveDate) -> Result<()> {
    fs::create_dir_all(archive.join("graphs"))?;
    let mut sorted = nodes.to_vec();
    sorted.sort_by_key(|n| n.start_at);

    let mermaid_path = day_mermaid_path(archive, date);
    let mermaid = to_mermaid(&sorted);
    fs::write(mermaid_path, mermaid)?;

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

fn parse_mermaid(content: &str, date: NaiveDate) -> Result<Vec<Session>> {
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
            let (name, goal_id, explicit_time) = split_label(label, date);
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
                    start_at,
                    end_at,
                });
            }
        }
        cursor = outgoing.get(&id).cloned();
    }
    Ok(nodes)
}

fn split_label(
    label: &str,
    date: NaiveDate,
) -> (String, u64, Option<(DateTime<Utc>, DateTime<Utc>)>) {
    let (without_time, time_range) = match label.rsplit_once('[') {
        Some((head, tail)) => (
            head.trim(),
            parse_time_range(tail.trim_end_matches(']').trim(), date),
        ),
        None => (label.trim(), None),
    };

    let name_with_id = without_time.trim();

    let mut goal_id = 0;
    let mut name = name_with_id.to_string();
    if let Some((name_only, id_part)) = name_with_id.rsplit_once("[id") {
        let parsed_id = id_part
            .trim_end_matches(']')
            .trim()
            .trim_start_matches(|c: char| c == ':' || c.is_whitespace())
            .parse::<u64>()
            .ok();
        if let Some(id_val) = parsed_id {
            goal_id = id_val;
            name = name_only.trim().to_string();
        }
    }

    (name, goal_id, time_range)
}

fn parse_time_range(range: &str, date: NaiveDate) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let normalized = range.replace("#colon;", ":");
    let (start_raw, end_raw) = normalized.split_once('-')?;
    let start_time = NaiveTime::parse_from_str(start_raw.trim(), "%H:%M").ok()?;
    let end_time = NaiveTime::parse_from_str(end_raw.trim(), "%H:%M").ok()?;

    let start_naive = NaiveDateTime::new(date, start_time);
    let mut end_naive = NaiveDateTime::new(date, end_time);
    if end_naive < start_naive {
        end_naive = end_naive + ChronoDuration::days(1);
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

fn to_mermaid(nodes: &[Session]) -> String {
    let mut out = String::from("stateDiagram-v2\n");
    if let Some(first) = nodes.first() {
        out.push_str(&format!("    [*] --> {}\n", first.id));
    }
    for (i, n) in nodes.iter().enumerate() {
        let times = format_time_range_for_mermaid(n);
        out.push_str(&format!(
            "    {}: {} [id {}] [{}]\n",
            n.id, n.name, n.goal_id, times
        ));
        if let Some(next) = nodes.get(i + 1) {
            out.push_str(&format!("    {} --> {}\n", n.id, next.id));
        }
    }
    out
}

fn format_time_range_for_mermaid(node: &Session) -> String {
    fn hhmm_encoded(dt: DateTime<Local>) -> String {
        dt.format("%H:%M").to_string().replace(':', "#colon;")
    }

    let start = hhmm_encoded(node.start_at.with_timezone(&Local));
    let end = hhmm_encoded(node.end_at.with_timezone(&Local));
    format!("{start}-{end}")
}
