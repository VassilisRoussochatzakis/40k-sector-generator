use thiserror::Error;

use sectorforge::MutationError;

#[derive(Debug, Error)]
pub enum BuilderError {
    #[error("io: {0}")]
    IoFailed(#[from] std::io::Error),
    #[error("parse error in {file}: {message}")]
    ParseFailed { file: String, message: String },
    #[error(transparent)]
    Mutation(#[from] MutationError),
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
}
