use std::path::{Path, PathBuf};

use rskit_codec::{Codec, YamlCodec};
use rskit_errors::{AppError, AppResult};

use super::contract::ConfigSource;
use super::toml::{json_value_to_config, read_optional_config_file};

/// YAML file source.
#[derive(Debug, Clone)]
pub struct YamlFileSource {
    path: PathBuf,
    required: bool,
}

impl YamlFileSource {
    /// Create a required YAML file source.
    pub fn required(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            required: true,
        }
    }

    /// Create an optional YAML file source.
    pub fn optional(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            required: false,
        }
    }

    /// Return the configured path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl ConfigSource for YamlFileSource {
    fn collect(&self) -> AppResult<config::Config> {
        let Some(contents) = read_optional_config_file(&self.path, self.required, "YAML")? else {
            return json_value_to_config(serde_json::json!({}));
        };
        let value = YamlCodec.decode_value(&contents).map_err(|err| {
            AppError::invalid_input(
                "config",
                format!(
                    "failed to load YAML config '{}': {err}",
                    self.path.display()
                ),
            )
            .with_cause(err)
        })?;
        json_value_to_config(value)
    }
}
