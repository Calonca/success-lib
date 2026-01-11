use std::fs::{self};
use std::path::{Path, PathBuf};

use anyhow::Result;

pub fn notes_path(archive: &Path, goal_id: u64) -> PathBuf {
    archive.join("notes").join(format!("goal_{goal_id}.md"))
}

pub fn get_note(archive: &Path, goal_id: u64) -> Result<String> {
    let path = notes_path(archive, goal_id);
    if !path.exists() {
        return Ok(String::new());
    }
    let content = fs::read_to_string(path)?;
    Ok(content)
}

pub fn edit_note(archive: &Path, goal_id: u64, content: &str) -> Result<()> {
    let path = notes_path(archive, goal_id);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let content_with_newline = if content.ends_with('\n') {
        content.to_string()
    } else {
        format!("{}\n", content)
    };
    fs::write(path, content_with_newline)?;
    Ok(())
}
