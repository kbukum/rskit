//! Logging configuration vocabulary.
//!
//! These are plain `serde` data types describing *what* logging should do —
//! the level, output format, and sink. They carry no `tracing` dependency and
//! are always available, even when the `setup` feature (the subscriber-building
//! layer) is disabled. This lets configuration crates compose the logging
//! vocabulary without linking the `tracing` subscriber stack.

use serde::Deserialize;

/// Logging configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    /// Minimum log level: `trace`, `debug`, `info`, `warn`, `error`.
    #[serde(default = "LoggingConfig::default_level")]
    pub level: String,

    /// Log output format (JSON or console).
    #[serde(default)]
    pub format: LogFormat,

    /// Override service name in log output (defaults to the service identity).
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
#[non_exhaustive]
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
#[non_exhaustive]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum LogOutput {
    /// Write to standard output (default).
    #[default]
    Stdout,
    /// Write to standard error.
    Stderr,
    /// Write to a file at the given path.
    File {
        /// Absolute or relative path to the log file.
        path: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

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
