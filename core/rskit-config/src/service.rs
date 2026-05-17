use rskit_validation::Validate;
use serde::{Deserialize, Serialize};

/// Base service configuration — embed this in every application config.
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct ServiceConfig {
    /// Service name used in logs, traces, and health responses.
    #[validate(length(min = 1))]
    #[serde(default = "ServiceConfig::default_name")]
    pub name: String,

    /// Deployment environment (development, staging, production).
    #[serde(default)]
    pub environment: Environment,

    /// Semver version string, defaults to the version provided by `rskit-version`.
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

/// Deployment environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    /// Local development environment (default).
    #[default]
    Development,
    /// Pre-production / staging environment.
    Staging,
    /// Live production environment.
    Production,
}

impl Environment {
    /// Returns `true` if this is the production environment.
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

    /// Log output format (JSON or console).
    #[serde(default)]
    pub format: LogFormat,

    /// Override service name in log output (defaults to [`ServiceConfig::name`]).
    pub service_name: Option<String>,

    /// Where to write log output.
    #[serde(default)]
    pub output: LogOutput,

    /// Include `file:line` caller location in every log line.
    #[serde(default)]
    pub with_caller: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: Self::default_level(),
            format: LogFormat::default(),
            service_name: None,
            output: LogOutput::default(),
            with_caller: false,
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

/// Where log output is written.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum LogOutput {
    /// Write to standard output (default).
    #[default]
    Stdout,
    /// Write to standard error.
    Stderr,
    /// Write to a file at the given path.
    /// Not yet implemented — falls back to stdout.
    File {
        /// Absolute or relative path to the log file.
        path: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Environment enum ────────────────────────────────────────────

    #[test]
    fn environment_display_development() {
        assert_eq!(Environment::Development.to_string(), "development");
    }

    #[test]
    fn environment_display_staging() {
        assert_eq!(Environment::Staging.to_string(), "staging");
    }

    #[test]
    fn environment_display_production() {
        assert_eq!(Environment::Production.to_string(), "production");
    }

    #[test]
    fn environment_default_is_development() {
        assert_eq!(Environment::default(), Environment::Development);
    }

    #[test]
    fn environment_is_production_returns_true_for_production() {
        assert!(Environment::Production.is_production());
    }

    #[test]
    fn environment_is_production_returns_false_for_development() {
        assert!(!Environment::Development.is_production());
    }

    #[test]
    fn environment_is_production_returns_false_for_staging() {
        assert!(!Environment::Staging.is_production());
    }

    #[test]
    fn environment_deserialize_from_lowercase_string() {
        let dev: Environment = serde_json::from_str(r#""development""#).unwrap();
        assert_eq!(dev, Environment::Development);

        let stg: Environment = serde_json::from_str(r#""staging""#).unwrap();
        assert_eq!(stg, Environment::Staging);

        let prd: Environment = serde_json::from_str(r#""production""#).unwrap();
        assert_eq!(prd, Environment::Production);
    }

    #[test]
    fn environment_deserialize_unknown_string_fails() {
        let result: Result<Environment, _> = serde_json::from_str(r#""unknown""#);
        assert!(result.is_err());
    }

    #[test]
    fn environment_clone_and_eq() {
        let env = Environment::Staging;
        let cloned = env.clone();
        assert_eq!(env, cloned);
    }

    // ── ServiceConfig ───────────────────────────────────────────────

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
        use rskit_validation::Validate;
        let cfg = ServiceConfig {
            name: String::new(),
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn service_config_validation_valid_name_passes() {
        use rskit_validation::Validate;
        let cfg = ServiceConfig::default();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn service_config_validation_long_name_passes() {
        use rskit_validation::Validate;
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

    // ── LoggingConfig ───────────────────────────────────────────────

    #[test]
    fn logging_config_default_level() {
        let cfg = LoggingConfig::default();
        assert_eq!(cfg.level, "info");
    }

    #[test]
    fn logging_config_default_format() {
        let cfg = LoggingConfig::default();
        assert_eq!(cfg.format, LogFormat::Console);
    }

    #[test]
    fn logging_config_default_output() {
        let cfg = LoggingConfig::default();
        assert_eq!(cfg.output, LogOutput::Stdout);
    }

    #[test]
    fn logging_config_default_service_name_is_none() {
        let cfg = LoggingConfig::default();
        assert!(cfg.service_name.is_none());
    }

    #[test]
    fn logging_config_default_with_caller_false() {
        let cfg = LoggingConfig::default();
        assert!(!cfg.with_caller);
    }

    // ── LogFormat ───────────────────────────────────────────────────

    #[test]
    fn log_format_default_is_console() {
        assert_eq!(LogFormat::default(), LogFormat::Console);
    }

    #[test]
    fn log_format_json_variant() {
        let fmt = LogFormat::Json;
        assert_ne!(fmt, LogFormat::Console);
    }

    #[test]
    fn log_format_deserialize_json() {
        let fmt: LogFormat = serde_json::from_str(r#""json""#).unwrap();
        assert_eq!(fmt, LogFormat::Json);
    }

    #[test]
    fn log_format_deserialize_console() {
        let fmt: LogFormat = serde_json::from_str(r#""console""#).unwrap();
        assert_eq!(fmt, LogFormat::Console);
    }

    // ── LogOutput ───────────────────────────────────────────────────

    #[test]
    fn log_output_default_is_stdout() {
        assert_eq!(LogOutput::default(), LogOutput::Stdout);
    }

    #[test]
    fn log_output_stderr_variant() {
        let out = LogOutput::Stderr;
        assert_ne!(out, LogOutput::Stdout);
    }

    #[test]
    fn log_output_file_variant() {
        let out = LogOutput::File {
            path: "/var/log/app.log".to_string(),
        };
        assert_eq!(
            out,
            LogOutput::File {
                path: "/var/log/app.log".to_string()
            }
        );
    }

    #[test]
    fn log_output_deserialize_stdout() {
        let out: LogOutput = serde_json::from_str(r#"{"type":"stdout"}"#).unwrap();
        assert_eq!(out, LogOutput::Stdout);
    }

    #[test]
    fn log_output_deserialize_stderr() {
        let out: LogOutput = serde_json::from_str(r#"{"type":"stderr"}"#).unwrap();
        assert_eq!(out, LogOutput::Stderr);
    }

    #[test]
    fn log_output_deserialize_file() {
        let out: LogOutput =
            serde_json::from_str(r#"{"type":"file","path":"/logs/app.log"}"#).unwrap();
        assert_eq!(
            out,
            LogOutput::File {
                path: "/logs/app.log".to_string()
            }
        );
    }
}
