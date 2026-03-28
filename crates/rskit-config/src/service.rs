use serde::{Deserialize, Serialize};
use validator::Validate;

/// Base service configuration — embed this in every application config.
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct ServiceConfig {
    /// Service name used in logs, traces, and health responses.
    #[validate(length(min = 1))]
    #[serde(default = "ServiceConfig::default_name")]
    pub name: String,

    #[serde(default)]
    pub environment: Environment,

    #[serde(default = "ServiceConfig::default_version")]
    pub version: String,

    #[serde(default)]
    pub debug: bool,

    #[serde(default)]
    pub logging: LoggingConfig,
}

impl ServiceConfig {
    fn default_name() -> String {
        "service".to_string()
    }
    fn default_version() -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            name: Self::default_name(),
            environment: Environment::default(),
            version: Self::default_version(),
            debug: false,
            logging: LoggingConfig::default(),
        }
    }
}

/// Deployment environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    #[default]
    Development,
    Staging,
    Production,
}

impl Environment {
    pub fn is_production(&self) -> bool {
        *self == Environment::Production
    }
}

impl std::fmt::Display for Environment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Environment::Development => f.write_str("development"),
            Environment::Staging => f.write_str("staging"),
            Environment::Production => f.write_str("production"),
        }
    }
}

/// Logging configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    /// Minimum log level: `trace`, `debug`, `info`, `warn`, `error`.
    #[serde(default = "LoggingConfig::default_level")]
    pub level: String,

    #[serde(default)]
    pub format: LogFormat,

    /// Override service name in log output (defaults to [`ServiceConfig::name`]).
    pub service_name: Option<String>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: Self::default_level(),
            format: LogFormat::default(),
            service_name: None,
        }
    }
}

impl LoggingConfig {
    fn default_level() -> String {
        "info".to_string()
    }
}

/// Log output format.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// Machine-readable JSON (use in production).
    Json,
    /// Human-readable coloured output (default, use in development).
    #[default]
    Console,
}
