use std::path::{Path, PathBuf};

use rskit_errors::{AppError, AppResult};

use super::contract::ConfigSource;

/// TOML file source.
#[derive(Debug, Clone)]
pub struct TomlFileSource {
    path: PathBuf,
    required: bool,
}

impl TomlFileSource {
    /// Create a required TOML file source.
    pub fn required(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            required: true,
        }
    }

    /// Create an optional TOML file source.
    pub fn optional(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            required: false,
        }
    }

    /// Return the configured path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl ConfigSource for TomlFileSource {
    fn collect(&self) -> AppResult<config::Config> {
        config::Config::builder()
            .add_source(config::File::from(self.path.as_path()).required(self.required))
            .build()
            .map_err(|e| {
                AppError::invalid_input(
                    "config",
                    format!("failed to load TOML config '{}': {e}", self.path.display()),
                )
            })
    }
}
