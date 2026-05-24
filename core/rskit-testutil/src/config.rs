//! Config test helpers.

use rskit_config::{AppConfig, ServiceConfig};
use serde::Deserialize;

/// Minimal application config for tests that need an `AppConfig`.
#[derive(Debug, Clone, Default, Deserialize, rskit_validation::Validate)]
pub struct TestAppConfig {
    /// Embedded service configuration.
    #[serde(default)]
    pub service: ServiceConfig,
}

impl TestAppConfig {
    /// Create a test config with a custom service name.
    #[must_use]
    pub fn named(name: impl Into<String>) -> Self {
        let service = ServiceConfig {
            name: name.into(),
            ..ServiceConfig::default()
        };
        Self { service }
    }
}

impl AppConfig for TestAppConfig {
    fn apply_defaults(&mut self) {}

    fn service_config(&self) -> &ServiceConfig {
        &self.service
    }
}
