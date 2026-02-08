use std::path::Path;

use crate::ffi_types::AppError;
use crate::storage_io;

pub fn notes_path(archive: &Path, goal_id: u64) -> std::path::PathBuf {
    archive.join("notes").join(format!("goal_{goal_id}.md"))
}

pub fn get_note(archive: &Path, goal_id: u64) -> Result<String, AppError> {
    let path = notes_path(archive, goal_id);
    Ok(storage_io::read_to_string(archive, &path)?.unwrap_or_default())
}

pub fn edit_note(archive: &Path, goal_id: u64, content: &str) -> Result<(), AppError> {
    let path = notes_path(archive, goal_id);
    let content_with_newline = if content.ends_with('\n') {
        content.to_string()
    } else {
        format!("{}\n", content)
    };
    storage_io::write_string(archive, &path, &content_with_newline)?;
    Ok(())
}
