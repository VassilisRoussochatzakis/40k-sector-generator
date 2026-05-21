//! Disk operations for the editor: list projects under `examples/`, load and
//! save sector JSON.

use std::fs;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EditorFileError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid project name: {0}")]
    InvalidProjectName(String),
}

const EXAMPLES_DIR: &str = "examples";

/// Discover project directories that contain `out/sector.json`.
pub fn list_projects() -> Vec<String> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(EXAMPLES_DIR) else {
        return out;
    };
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if !ft.is_dir() {
            continue;
        }
        let json = entry.path().join("out").join("sector.json");
        if json.exists() {
            if let Some(name) = entry.file_name().to_str() {
                out.push(name.to_string());
            }
        }
    }
    out.sort();
    out
}

pub fn project_sector_path(name: &str) -> PathBuf {
    PathBuf::from(EXAMPLES_DIR)
        .join(name)
        .join("out")
        .join("sector.json")
}

pub fn load_project_sector(
    name: &str,
) -> Result<(crate::sector_model::GeneratedSector, String), EditorFileError> {
    let path = project_sector_path(name);
    let text = fs::read_to_string(&path)?;
    let sector: crate::sector_model::GeneratedSector = serde_json::from_str(&text)?;
    Ok((sector, path.to_string_lossy().to_string()))
}

pub fn save_project_sector(
    name: &str,
    sector: &crate::sector_model::GeneratedSector,
) -> Result<String, EditorFileError> {
    if name.trim().is_empty() {
        return Err(EditorFileError::InvalidProjectName(
            "project name is empty".to_string(),
        ));
    }
    if name.contains('/') || name.contains('\\') || name == "." || name == ".." {
        return Err(EditorFileError::InvalidProjectName(
            "project name contains forbidden characters".to_string(),
        ));
    }
    let path = project_sector_path(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(sector)?;
    fs::write(&path, text)?;
    Ok(path.to_string_lossy().to_string())
}
