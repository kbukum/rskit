//! Skill error types and application-error conversion.

use std::path::PathBuf;

use rskit_errors::AppError;
use thiserror::Error;

/// Skill errors.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SkillError {
    /// File I/O failed.
    #[error("file I/O failed for {path}: {source}")]
    Io {
        /// File path.
        path: PathBuf,
        /// Source error.
        #[source]
        source: std::io::Error,
    },
    /// YAML parsing failed.
    #[error("manifest parse failed for {path}: {source}")]
    ParseManifest {
        /// Manifest path.
        path: PathBuf,
        /// Source error.
        #[source]
        source: serde_norway::Error,
    },
    /// Manifest is invalid.
    #[error("invalid skill manifest: {0}")]
    InvalidManifest(String),
    /// Config source resolution or validation failed.
    #[error("skill config failed: {0}")]
    Config(String),
    /// Verification failed.
    #[error("skill verification failed: {0}")]
    Verification(String),
    /// Registry conflict.
    #[error("skill already registered: {0}")]
    AlreadyRegistered(String),
    /// Skill not found.
    #[error("skill not found: {0}")]
    NotFound(String),
}

impl From<SkillError> for AppError {
    fn from(value: SkillError) -> Self {
        match value {
            SkillError::NotFound(name) => AppError::not_found("skill", Some(name.as_str())),
            SkillError::AlreadyRegistered(name) => {
                AppError::already_exists(format!("skill {name}"))
            }
            SkillError::InvalidManifest(message)
            | SkillError::Config(message)
            | SkillError::Verification(message) => AppError::invalid_input("skill", message),
            SkillError::Io { path, source } => AppError::new(
                rskit_errors::ErrorCode::Internal,
                format!("skill I/O failed for {}: {source}", path.display()),
            )
            .with_cause(source),
            SkillError::ParseManifest { path, source } => AppError::new(
                rskit_errors::ErrorCode::InvalidInput,
                format!(
                    "skill manifest parse failed for {}: {source}",
                    path.display()
                ),
            )
            .with_cause(source),
        }
    }
}
