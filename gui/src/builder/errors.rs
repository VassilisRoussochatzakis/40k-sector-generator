use thiserror::Error;

use sectorforge::sector_model::mutation::MutationError;

#[derive(Debug, Error)]
pub enum BuilderError {
    #[error("validation failed: {0}")]
    ValidationFailed(String),
    #[error("invariant violated: {0}")]
    InvariantViolated(String),
    #[error("io: {0}")]
    IoFailed(#[from] std::io::Error),
    #[error("parse error in {file}: {message}")]
    ParseFailed { file: String, message: String },
    #[error("entity not found: {0}")]
    EntityNotFound(String),
    #[error("stale snapshot: {0}")]
    StaleSnapshot(String),
    #[error(transparent)]
    Mutation(#[from] MutationError),
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
}
