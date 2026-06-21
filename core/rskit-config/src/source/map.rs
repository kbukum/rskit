use rskit_errors::{AppError, AppResult};

use super::contract::ConfigSource;

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
