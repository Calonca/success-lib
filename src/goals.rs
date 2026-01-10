use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

use crate::types::{Goal, GoalStatus};

const DEFAULT_VISIBLE_STATUSES: [GoalStatus; 2] = [GoalStatus::TODO, GoalStatus::DOING];

fn goals_path(archive: &Path) -> PathBuf {
    archive.join("goals.yaml")
}

fn read_goals(archive: &Path) -> Result<Vec<Goal>> {
    let path = goals_path(archive);
    if !path.exists() {
        return Ok(vec![]);
    }
    let data = fs::read_to_string(&path)?;
    let goals: Vec<Goal> = serde_yaml::from_str(&data).unwrap_or_default();
    Ok(goals)
}

fn write_goals(archive: &Path, goals: &[Goal]) -> Result<()> {
    let path = goals_path(archive);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let data = serde_yaml::to_string(goals)?;
    fs::write(path, data)?;
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

pub fn list_goals(archive: &Path, statuses: Option<&[GoalStatus]>) -> Result<Vec<Goal>> {
    let goals = read_goals(archive)?;
    Ok(filter_goals(goals, statuses))
}

pub fn list_trash(archive: &Path) -> Result<Vec<Goal>> {
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
) -> Result<Goal> {
    let mut goals = read_goals(archive)?;
    let id = next_goal_id(&goals);
    let goal = Goal {
        id,
        name: name.to_string(),
        is_reward,
        commands,
        status: GoalStatus::TODO,
        trashed: false,
    };
    goals.push(goal.clone());

    write_goals(archive, &goals)?;

    Ok(goal)
}

pub fn set_goal_status(archive: &Path, goal_id: u64, status: GoalStatus) -> Result<Goal> {
    let mut goals = read_goals(archive)?;
    let mut updated_goal = None;

    for goal in &mut goals {
        if goal.id == goal_id {
            goal.status = status;
            updated_goal = Some(goal.clone());
            break;
        }
    }

    let goal = updated_goal.ok_or_else(|| anyhow!("Goal {goal_id} not found"))?;
    write_goals(archive, &goals)?;

    Ok(goal)
}

pub fn set_goal_trashed(archive: &Path, goal_id: u64, trashed: bool) -> Result<Goal> {
    let mut goals = read_goals(archive)?;
    let mut updated_goal = None;

    for goal in &mut goals {
        if goal.id == goal_id {
            goal.trashed = trashed;
            updated_goal = Some(goal.clone());
            break;
        }
    }

    let goal = updated_goal.ok_or_else(|| anyhow!("Goal {goal_id} not found"))?;
    write_goals(archive, &goals)?;

    Ok(goal)
}

pub fn search_goals(
    archive: &Path,
    query: &str,
    is_reward: Option<bool>,
    statuses: Option<&[GoalStatus]>,
) -> Result<Vec<Goal>> {
    let goals = list_goals(archive, statuses)?;
    let matcher = SkimMatcherV2::default();
    let trimmed = query.trim();

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

    scored.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(scored.into_iter().map(|(_, g)| g).collect())
}
