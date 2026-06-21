use rskit_validation::Validate;
use serde::Deserialize;
use validator::{ValidationError, ValidationErrors};

use super::environment::Environment;
use rskit_logging::LoggingConfig;

/// Base service configuration — embed this in every application config.
#[derive(Debug, Clone, Deserialize)]
pub struct ServiceConfig {
    /// Service name used in logs, traces, and health responses.
    #[serde(default = "ServiceConfig::default_name")]
    pub name: String,

    /// Deployment environment (development, staging, production).
    #[serde(default)]
    pub environment: Environment,

    /// Semver version string, defaults to `CARGO_PKG_VERSION`.
    #[serde(default = "ServiceConfig::default_version")]
    pub version: String,

    /// Network address the service binds to.
    #[serde(default = "ServiceConfig::default_address")]
    pub address: String,

    /// Network port the service listens on.
    #[serde(default = "ServiceConfig::default_port")]
    pub port: u16,

    /// Enable verbose debug output.
    #[serde(default)]
    pub debug: bool,

    /// Logging configuration.
    #[serde(default)]
    pub logging: LoggingConfig,
}

impl Validate for ServiceConfig {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        if self.name.trim().is_empty() {
            errors.add("name", ValidationError::new("length"));
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl ServiceConfig {
    fn default_name() -> String {
        "service".to_string()
    }
    fn default_version() -> String {
        rskit_version::package_version().to_string()
    }
    fn default_address() -> String {
        "0.0.0.0".to_string()
    }
    fn default_port() -> u16 {
        50051
    }
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            name: Self::default_name(),
            environment: Environment::default(),
            version: Self::default_version(),
            address: Self::default_address(),
            port: Self::default_port(),
            debug: false,
            logging: LoggingConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::{LogFormat, LogOutput};

    #[test]
    fn service_config_default_name() {
        let cfg = ServiceConfig::default();
        assert_eq!(cfg.name, "service");
    }

    #[test]
    fn service_config_default_environment() {
        let cfg = ServiceConfig::default();
        assert_eq!(cfg.environment, Environment::Development);
    }

    #[test]
    fn service_config_default_debug_false() {
        let cfg = ServiceConfig::default();
        assert!(!cfg.debug);
    }

    #[test]
    fn service_config_default_version_is_cargo_pkg() {
        let cfg = ServiceConfig::default();
        assert_eq!(cfg.version, rskit_version::package_version());
    }

    #[test]
    fn service_config_default_logging() {
        let cfg = ServiceConfig::default();
        assert_eq!(cfg.logging.level, "info");
        assert_eq!(cfg.logging.format, LogFormat::Console);
        assert_eq!(cfg.logging.output, LogOutput::Stdout);
    }

    #[test]
    fn service_config_validation_empty_name_fails() {
        let cfg = ServiceConfig {
            name: String::new(),
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn service_config_validation_valid_name_passes() {
        let cfg = ServiceConfig::default();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn service_config_validation_long_name_passes() {
        let cfg = ServiceConfig {
            name: "a".repeat(1000),
            ..Default::default()
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn service_config_all_fields_accessible() {
        let cfg = ServiceConfig::default();
        let _ = &cfg.name;
        let _ = &cfg.environment;
        let _ = &cfg.version;
        let _ = &cfg.address;
        let _ = cfg.port;
        let _ = cfg.debug;
        let _ = &cfg.logging;
    }

    #[test]
    fn service_config_default_address() {
        let cfg = ServiceConfig::default();
        assert_eq!(cfg.address, "0.0.0.0");
    }

    #[test]
    fn service_config_default_port() {
        let cfg = ServiceConfig::default();
        assert_eq!(cfg.port, 50051);
    }

    #[test]
    fn service_config_debug_format() {
        let cfg = ServiceConfig::default();
        let debug_str = format!("{:?}", cfg);
        assert!(debug_str.contains("ServiceConfig"));
        assert!(debug_str.contains("service"));
    }
}
