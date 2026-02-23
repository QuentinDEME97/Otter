use crate::error::{AppError, AppResult};
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;

pub fn validate_relative_file_path(path: &str) -> AppResult<()> {
    if path.trim().is_empty() {
        return Err(AppError::BadRequest(
            "File path cannot be empty".to_string(),
        ));
    }

    let candidate = Path::new(path);
    if candidate.is_absolute() {
        return Err(AppError::BadRequest(
            "File path must be relative".to_string(),
        ));
    }

    for component in candidate.components() {
        match component {
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(AppError::BadRequest(
                    "File path contains invalid traversal segments".to_string(),
                ));
            }
            _ => {}
        }
    }

    Ok(())
}

pub fn file_storage_path(data_dir: &Path, file_id: Uuid) -> PathBuf {
    data_dir.join(format!("{file_id}.bin"))
}
