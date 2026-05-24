//! Crate-wide error type and Result alias.

use std::path::PathBuf;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, FlaketideError>;

#[derive(Debug, Error)]
pub enum FlaketideError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("config error: {0}")]
    Config(String),

    #[error("config file not found at any of: {0:?}")]
    ConfigNotFound(Vec<PathBuf>),

    #[error("toml deserialization error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("parse error: {0}")]
    Parse(String),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("xml error: {0}")]
    Xml(String),

    #[error("could not detect test framework; pass --framework or set it in flaketide.toml")]
    FrameworkUndetected,

    #[error("unknown framework: {0}")]
    UnknownFramework(String),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("migration error: {0}")]
    Migration(#[from] rusqlite_migration::Error),

    #[error("runner error: {0}")]
    Runner(String),

    #[error("test command timed out after {0:?}")]
    Timeout(std::time::Duration),

    #[error("ai service error: {0}")]
    Ai(String),

    #[error("ai disabled: ANTHROPIC_API_KEY is not set")]
    AiDisabled,

    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("github error: {0}")]
    GitHub(String),

    #[error("invariant violation: {0}")]
    Invariant(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl FlaketideError {
    /// Map errors to documented process exit codes.
    pub fn exit_code(&self) -> i32 {
        match self {
            FlaketideError::Config(_)
            | FlaketideError::ConfigNotFound(_)
            | FlaketideError::Toml(_)
            | FlaketideError::FrameworkUndetected
            | FlaketideError::UnknownFramework(_) => 2,
            FlaketideError::AiDisabled => 3,
            _ => 1,
        }
    }
}
