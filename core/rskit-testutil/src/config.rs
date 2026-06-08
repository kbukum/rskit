//! Config test helpers.

use rskit_config::{AppConfig, ServiceConfig};
use serde::Deserialize;
use validator::{ValidationError, ValidationErrors};

/// Minimal application config for tests that need an `AppConfig`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TestAppConfig {
    /// Embedded service configuration.
    #[serde(default)]
    pub service: ServiceConfig,
}

impl rskit_validation::Validate for TestAppConfig {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        if let Err(error) = rskit_validation::Validate::validate(&self.service) {
            let mut validation_error = ValidationError::new("invalid_service");
            validation_error.message = Some(error.to_string().into());
            errors.add("service", validation_error);
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
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
