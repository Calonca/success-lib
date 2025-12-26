use std::path::Path;

use anyhow::Result;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

use crate::types::Goal;

pub fn list_goals(archive: &Path) -> Result<Vec<Goal>> {
    let path = archive.join("goals.yaml");
    if !path.exists() {
        return Ok(vec![]);
    }
    let data = std::fs::read_to_string(&path)?;
    let goals: Vec<Goal> = serde_yaml::from_str(&data).unwrap_or_default();
    Ok(goals)
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
    let mut goals = list_goals(archive)?;
    let id = next_goal_id(&goals);
    let goal = Goal {
        id,
        name: name.to_string(),
        is_reward,
        commands,
    };
    goals.push(goal.clone());

    let path = archive.join("goals.yaml");
    let data = serde_yaml::to_string(&goals)?;
    std::fs::write(path, data)?;

    Ok(goal)
}

pub fn search_goals(archive: &Path, query: &str, is_reward: Option<bool>) -> Result<Vec<Goal>> {
    let goals = list_goals(archive)?;
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
