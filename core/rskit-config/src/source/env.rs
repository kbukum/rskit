use rskit_errors::{AppError, AppResult};

use super::contract::ConfigSource;

/// Environment-variable configuration source.
#[derive(Debug, Clone, Default)]
pub struct EnvironmentSource {
    prefix: String,
}

impl EnvironmentSource {
    /// Create an environment source without a prefix.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an environment source with a prefix.
    pub fn with_prefix(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
        }
    }
}

impl ConfigSource for EnvironmentSource {
    fn collect(&self) -> AppResult<config::Config> {
        let env_source = if self.prefix.is_empty() {
            config::Environment::default()
                .separator("__")
                .try_parsing(true)
        } else {
            config::Environment::with_prefix(&self.prefix)
                .separator("__")
                .try_parsing(true)
        };

        config::Config::builder()
            .add_source(env_source)
            .build()
            .map_err(|e| AppError::invalid_input("config", e.to_string()))
    }
}

pub(crate) fn normalize_env_key(prefix: &str, key: &str) -> Option<String> {
    let key = key.to_ascii_lowercase();
    if prefix.is_empty() {
        return Some(key.replace("__", "."));
    }

    let prefix = prefix.to_ascii_lowercase();
    key.strip_prefix(&format!("{prefix}__"))
        .map(|stripped| stripped.replace("__", "."))
}

pub(crate) fn parse_env_value(value: String) -> config::Value {
    if let Ok(value) = value.parse::<bool>() {
        return value.into();
    }
    if let Ok(value) = value.parse::<i64>() {
        return value.into();
    }
    if let Ok(value) = value.parse::<f64>() {
        return value.into();
    }
    value.into()
}
