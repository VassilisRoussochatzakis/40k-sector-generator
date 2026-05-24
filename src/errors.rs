use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SectorError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse config at {path}: {message}")]
    ConfigParse { path: String, message: String },

    #[error("failed to load world data at {path}: {message}")]
    WorldDataLoad { path: String, message: String },

    #[error("validation failed: {error_count} error(s), {warning_count} warning(s)")]
    ValidationFailed {
        error_count: usize,
        warning_count: usize,
    },

    #[error("no usable world candidates were found in the workbook")]
    NoWorldCandidates,

    #[error("generation cancelled")]
    GenerationCancelled,

    #[error("weighted selection failed for {context}")]
    WeightedSelectionFailed { context: String },

    #[error("export failed for {path}: {message}")]
    ExportFailed { path: String, message: String },

    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
}

#[derive(Debug, Error, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationError {
    #[error("entity not found: {0}")]
    NotFound(String),
    #[error("id collision: {0}")]
    Collision(String),
    #[error("invalid coordinate: {q}, {r}")]
    InvalidCoord { q: i32, r: i32 },
    #[error("invalid state: {0}")]
    InvalidState(String),
}

impl SectorError {
    pub fn io(path: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    pub fn config_parse(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self::ConfigParse {
            path: path.into(),
            message: message.into(),
        }
    }

    pub fn export(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self::ExportFailed {
            path: path.into(),
            message: message.into(),
        }
    }
}
