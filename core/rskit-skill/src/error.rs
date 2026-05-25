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
    /// File exceeded the configured skill-pack size limit.
    #[error("skill file {path} exceeds size limit of {limit_bytes} bytes")]
    FileTooLarge {
        /// File path.
        path: PathBuf,
        /// Maximum accepted size in bytes.
        limit_bytes: u64,
    },
    /// Aggregate skill-pack assets exceeded the configured size limit.
    #[error(
        "skill assets exceed total size limit of {limit_bytes} bytes while reading {path} ({total_bytes} bytes)"
    )]
    AssetsTooLarge {
        /// Asset path being read when the aggregate limit was exceeded.
        path: PathBuf,
        /// Bytes read across all assets.
        total_bytes: u64,
        /// Maximum accepted aggregate asset size in bytes.
        limit_bytes: u64,
    },
    /// A skill-pack text file is not valid UTF-8.
    #[error("skill file {path} is not valid UTF-8: {source}")]
    InvalidUtf8 {
        /// File path.
        path: PathBuf,
        /// Source error.
        #[source]
        source: std::string::FromUtf8Error,
    },
    /// A skill-pack path is not an accepted regular pack file or directory.
    #[error("invalid skill pack file {path}: {reason}")]
    InvalidPackFile {
        /// File path.
        path: PathBuf,
        /// Rejection reason.
        reason: String,
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
            SkillError::FileTooLarge { path, limit_bytes } => AppError::invalid_input(
                "skill",
                format!(
                    "file {} exceeds size limit of {limit_bytes} bytes",
                    path.display()
                ),
            ),
            SkillError::AssetsTooLarge {
                path,
                total_bytes,
                limit_bytes,
            } => AppError::invalid_input(
                "skill",
                format!(
                    "assets exceed total size limit of {limit_bytes} bytes while reading {} ({total_bytes} bytes)",
                    path.display()
                ),
            ),
            SkillError::InvalidUtf8 { path, source } => AppError::invalid_input(
                "skill",
                format!("file {} is not valid UTF-8: {source}", path.display()),
            )
            .with_cause(source),
            SkillError::InvalidPackFile { path, reason } => {
                AppError::invalid_input("skill", format!("{}: {reason}", path.display()))
            }
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use rskit_errors::{AppError, ErrorCode};

    use super::SkillError;

    #[test]
    fn aggregate_asset_limit_has_distinct_message_and_app_error() {
        let error = SkillError::AssetsTooLarge {
            path: PathBuf::from("references/large.bin"),
            total_bytes: 65,
            limit_bytes: 64,
        };

        assert_eq!(
            error.to_string(),
            "skill assets exceed total size limit of 64 bytes while reading references/large.bin (65 bytes)"
        );

        let app_error = AppError::from(error);
        assert_eq!(app_error.code(), ErrorCode::InvalidInput);
        assert_eq!(
            app_error.message(),
            "invalid skill: assets exceed total size limit of 64 bytes while reading references/large.bin (65 bytes)"
        );
    }
}
