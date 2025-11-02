//! Error types for FC CI

use thiserror::Error;

#[derive(Error, Debug)]
pub enum CiError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Git error: {0}")]
    Git(#[from] git2::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Build error: {0}")]
    Build(String),

    #[error("Not found: {0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, CiError>;
