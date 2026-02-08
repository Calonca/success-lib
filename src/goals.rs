use std::collections::HashMap;
use std::path::Path;

use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

use crate::ffi_types::AppError;
use crate::session_graph::list_sessions_between_dates;
use crate::storage_io;
use crate::types::{Goal, GoalStatus};

const DEFAULT_VISIBLE_STATUSES: [GoalStatus; 2] = [GoalStatus::TODO, GoalStatus::DOING];

fn goals_path(archive: &Path) -> std::path::PathBuf {
    archive.join("goals.yaml")
}

fn read_goals(archive: &Path) -> Result<Vec<Goal>, AppError> {
    let path = goals_path(archive);
    let Some(data) = storage_io::read_to_string(archive, &path)? else {
        return Ok(vec![]);
    };
    let goals: Vec<Goal> = serde_yaml::from_str(&data)?;
    Ok(goals)
}

fn write_goals(archive: &Path, goals: &[Goal]) -> Result<(), AppError> {
    let path = goals_path(archive);
    let data = serde_yaml::to_string(goals)?;
    storage_io::write_string(archive, &path, &data)?;
    Ok(())
}

fn filter_goals(goals: Vec<Goal>, statuses: Option<&[GoalStatus]>) -> Vec<Goal> {
    let statuses = statuses.unwrap_or(&DEFAULT_VISIBLE_STATUSES);
    goals
        .into_iter()
        .filter(|g| !g.trashed)
        .filter(|g| statuses.contains(&g.status))
        .collect()
}

pub fn list_goals(archive: &Path, statuses: Option<&[GoalStatus]>) -> Result<Vec<Goal>, AppError> {
    let goals = read_goals(archive)?;
    Ok(filter_goals(goals, statuses))
}

pub fn list_trash(archive: &Path) -> Result<Vec<Goal>, AppError> {
    let goals = read_goals(archive)?;
    Ok(goals.into_iter().filter(|g| g.trashed).collect())
}

pub fn next_goal_id(goals: &[Goal]) -> u64 {
    goals.iter().map(|g| g.id).max().unwrap_or(0) + 1
}

pub fn add_goal(
    archive: &Path,
    name: &str,
    is_reward: bool,
    commands: Vec<String>,
    quantity_name: Option<String>,
) -> Result<Goal, AppError> {
    let mut goals = read_goals(archive)?;
    let id = next_goal_id(&goals);
    let goal = Goal {
        id,
        name: name.to_string(),
        is_reward,
        commands,
        status: GoalStatus::TODO,
        trashed: false,
        quantity_name,
    };
    goals.push(goal.clone());

    write_goals(archive, &goals)?;

    Ok(goal)
}

pub fn set_goal_status(
    archive: &Path,
    goal_id: u64,
    status: GoalStatus,
) -> Result<Goal, AppError> {
    let mut goals = read_goals(archive)?;
    let mut updated_goal = None;

    for goal in &mut goals {
        if goal.id == goal_id {
            goal.status = status;
            updated_goal = Some(goal.clone());
            break;
        }
    }

    let goal = updated_goal.ok_or_else(|| AppError::NotFound {
        resource: "goal".into(),
        id: goal_id.to_string(),
    })?;
    write_goals(archive, &goals)?;

    Ok(goal)
}

pub fn set_goal_trashed(archive: &Path, goal_id: u64, trashed: bool) -> Result<Goal, AppError> {
    let mut goals = read_goals(archive)?;
    let mut updated_goal = None;

    for goal in &mut goals {
        if goal.id == goal_id {
            goal.trashed = trashed;
            updated_goal = Some(goal.clone());
            break;
        }
    }

    let goal = updated_goal.ok_or_else(|| AppError::NotFound {
        resource: "goal".into(),
        id: goal_id.to_string(),
    })?;
    write_goals(archive, &goals)?;

    Ok(goal)
}

pub fn get_goal(archive: &Path, goal_id: u64) -> Result<Goal, AppError> {
    let goals = read_goals(archive)?;
    goals.into_iter().find(|g| g.id == goal_id).ok_or_else(|| {
        AppError::NotFound {
            resource: "goal".into(),
            id: goal_id.to_string(),
        }
    })
}

pub fn search_goals(
    archive: &Path,
    query: &str,
    is_reward: Option<bool>,
    statuses: Option<&[GoalStatus]>,
    sort_by_recent: bool,
) -> Result<Vec<Goal>, AppError> {
    let goals = list_goals(archive, statuses)?;
    let matcher = SkimMatcherV2::default();
    let trimmed = query.trim();

    let recent_sessions = if sort_by_recent {
        list_sessions_between_dates(archive, None, None).unwrap_or_default()
    } else {
        vec![]
    };

    let mut last_active = HashMap::new();
    if sort_by_recent {
        for session in recent_sessions {
            let entry = last_active.entry(session.goal_id).or_insert(0);
            let ts = session.start_at.timestamp();
            if ts > *entry {
                *entry = ts;
            }
        }
    }

    let mut scored: Vec<(i64, Goal)> = goals
        .into_iter()
        .filter(|g| match is_reward {
            Some(flag) => g.is_reward == flag,
            None => true,
        })
        .filter_map(|g| {
            if trimmed.is_empty() {
                Some((0, g))
            } else {
                matcher.fuzzy_match(&g.name, trimmed).map(|s| (s, g))
            }
        })
        .collect();

    scored.sort_by(|(score_a, goal_a), (score_b, goal_b)| {
        if sort_by_recent {
            let active_a = last_active.get(&goal_a.id);
            let active_b = last_active.get(&goal_b.id);
            match (active_a, active_b) {
                (Some(ts_a), Some(ts_b)) => {
                    let cmp = ts_b.cmp(ts_a);
                    if cmp != std::cmp::Ordering::Equal {
                        return cmp;
                    }
                }
                (Some(_), None) => return std::cmp::Ordering::Less,
                (None, Some(_)) => return std::cmp::Ordering::Greater,
                _ => {}
            }
        }
        score_b.cmp(score_a)
    });
    Ok(scored.into_iter().map(|(_, g)| g).collect())
}
