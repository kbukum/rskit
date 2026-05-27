use std::fmt;
use std::path::{Path, PathBuf};

use rskit_errors::{AppError, AppResult};

/// Adapter contract for configuration sources.
///
/// Implement this trait in opt-in backend crates such as a future Vault,
/// Parameter Store, or remote-config adapter. `rskit-config` owns ordering,
/// decoding, defaults, and validation; adapters only return collected values.
pub trait ConfigSource: fmt::Debug + Send + Sync + 'static {
    /// Collect this source into a `config` source object.
    fn collect(&self) -> AppResult<config::Config>;
}

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

/// In-memory key/value config source.
#[derive(Debug, Clone, Default)]
pub struct ConfigMapSource {
    values: Vec<(String, config::Value)>,
}

impl ConfigMapSource {
    /// Create an empty in-memory source.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a value to the source.
    #[must_use]
    pub fn with_value(mut self, key: impl Into<String>, value: impl Into<config::Value>) -> Self {
        self.values.push((key.into(), value.into()));
        self
    }
}

impl ConfigSource for ConfigMapSource {
    fn collect(&self) -> AppResult<config::Config> {
        let mut builder = config::Config::builder();
        for (key, value) in &self.values {
            builder = builder
                .set_override(key, value.clone())
                .map_err(|e| AppError::invalid_input("config", e.to_string()))?;
        }
        builder
            .build()
            .map_err(|e| AppError::invalid_input("config", e.to_string()))
    }
}
