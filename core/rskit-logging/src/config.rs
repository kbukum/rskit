//! Logging configuration vocabulary.
//!
//! These are plain `serde` data types describing *what* logging should do — the level, output format,
//! and sink. They carry no `tracing` dependency and are always available,
//! even when the `setup` feature (the subscriber-building layer) is disabled.
//! This lets configuration crates compose the logging vocabulary without linking the `tracing` subscriber stack.

use std::collections::HashMap;

use serde::{Deserialize, Deserializer};

/// Logging configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    /// Minimum log level: `trace`, `debug`, `info`, `warn`, `error`.
    #[serde(default = "LoggingConfig::default_level")]
    pub level: String,

    /// Log output format (JSON or console).
    #[serde(default)]
    pub format: LogFormat,

    /// Disable ANSI colour output.
    #[serde(default)]
    pub no_color: bool,

    /// Include timestamps in log lines.
    #[serde(default = "default_true")]
    pub timestamp: bool,

    /// Override the service name reported to telemetry (the OTLP resource `service.name`).
    ///
    /// When set, this takes precedence over the base service identity passed to `init_logging_full`.
    pub service_name: Option<String>,

    /// Deployment environment reported to telemetry (the OTLP resource `deployment.environment`).
    ///
    /// When set, this takes precedence over the base environment passed to `init_logging_full`.
    pub environment: Option<String>,

    /// Service version reported to telemetry (the OTLP resource `service.version`).
    ///
    /// When set, this takes precedence over the base version passed to `init_logging_full`.
    pub version: Option<String>,

    /// Where to write log output.
    #[serde(default)]
    pub output: LogOutput,

    /// Include `file:line` caller location in every log line.
    #[serde(default, rename = "caller")]
    pub with_caller: bool,

    /// Sensitive-data masking configuration.
    #[serde(default)]
    pub masking: MaskingConfig,

    /// Rate-based sampling configuration.
    #[serde(default)]
    pub sampling: SamplingConfig,

    /// Per-module log-level overrides.
    #[serde(default)]
    pub module_levels: HashMap<String, String>,

    /// OTLP log export configuration.
    #[serde(default)]
    pub otlp: OtlpConfig,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: Self::default_level(),
            format: LogFormat::default(),
            no_color: false,
            timestamp: true,
            service_name: None,
            environment: None,
            version: None,
            output: LogOutput::default(),
            with_caller: false,
            masking: MaskingConfig::default(),
            sampling: SamplingConfig::default(),
            module_levels: HashMap::new(),
            otlp: OtlpConfig::default(),
        }
    }
}

fn default_true() -> bool {
    true
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
    /// Plain text output.
    Text,
    /// Pretty development output.
    Pretty,
}

/// Where log output is written.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
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

impl<'de> Deserialize<'de> for LogOutput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire {
            Shorthand(String),
            Tagged {
                #[serde(rename = "type")]
                kind: String,
                path: Option<String>,
            },
        }

        match Wire::deserialize(deserializer)? {
            Wire::Shorthand(kind) => parse_log_output_kind(&kind, None),
            Wire::Tagged { kind, path } => parse_log_output_kind(&kind, path),
        }
        .map_err(serde::de::Error::custom)
    }
}

fn parse_log_output_kind(kind: &str, path: Option<String>) -> Result<LogOutput, String> {
    match kind {
        "stdout" => Ok(LogOutput::Stdout),
        "stderr" => Ok(LogOutput::Stderr),
        "file" => path
            .map(|path| LogOutput::File { path })
            .ok_or_else(|| "file log output requires path".to_string()),
        other => Err(format!("unsupported log output type '{other}'")),
    }
}

/// Configuration for sensitive data masking.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct MaskingConfig {
    /// Whether masking is enabled. Default: `true`.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Additional field names to mask (beyond defaults).
    #[serde(default)]
    pub field_names: Vec<String>,

    /// Additional regex patterns for value masking.
    #[serde(default)]
    pub value_patterns: Vec<String>,

    /// Replacement string for field-name masking. Default: `"[REDACTED]"`.
    #[serde(default = "default_replacement")]
    pub replacement: String,

    /// Preserve this many trailing characters when masking whole field values.
    #[serde(default)]
    pub preserve_last: usize,
}

fn default_replacement() -> String {
    "[REDACTED]".to_string()
}

impl Default for MaskingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            field_names: Vec::new(),
            value_patterns: Vec::new(),
            replacement: default_replacement(),
            preserve_last: 0,
        }
    }
}

/// Configuration for log sampling.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct SamplingConfig {
    /// Master switch — when `false` the layer passes all events through.
    #[serde(default)]
    pub enabled: bool,
    /// Allow the first N events per second per level before sampling kicks in.
    #[serde(default = "default_sampling_rate")]
    pub initial_rate: u32,
    /// After the burst, allow every Nth event (1 = keep all, 2 = keep 50 %).
    #[serde(default = "default_sampling_rate")]
    pub thereafter_rate: u32,
}

fn default_sampling_rate() -> u32 {
    100
}

impl Default for SamplingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            initial_rate: 100,
            thereafter_rate: 100,
        }
    }
}

/// Per-module log level overrides.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct ModuleLevelsConfig {
    /// Module name → minimum level (e.g. `{"sqlx": "warn", "rdkafka": "off"}`).
    pub levels: HashMap<String, String>,
}

/// Configuration for OTLP log export.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct OtlpConfig {
    /// Master switch.
    #[serde(default)]
    pub enabled: bool,
    /// Collector endpoint. A scheme-less value takes its scheme from `insecure`; an explicit
    /// scheme must match `insecure` (`https://` secure, `http://` insecure).
    #[serde(default = "default_otlp_endpoint")]
    pub endpoint: String,
    /// Protocol: `"grpc"` or `"http"`.
    #[serde(default = "default_otlp_protocol")]
    pub protocol: String,
    /// Permit plaintext (`http://`) collector transport. Secure (`https://`) by default; an
    /// `http://` endpoint requires this to be `true`.
    #[serde(default)]
    pub insecure: bool,
    /// Additional HTTP headers.
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

fn default_otlp_endpoint() -> String {
    "localhost:4317".to_string()
}

fn default_otlp_protocol() -> String {
    "grpc".to_string()
}

impl Default for OtlpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: default_otlp_endpoint(),
            protocol: default_otlp_protocol(),
            insecure: false,
            headers: HashMap::new(),
        }
    }
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
    fn logging_config_deserializes_full_wire_surface() {
        let cfg: LoggingConfig = serde_json::from_str(
            r#"{
                "level": "trace",
                "format": "text",
                "output": {"type": "file", "path": "app.log"},
                "no_color": true,
                "timestamp": false,
                "caller": true,
                "service_name": "svc",
                "environment": "production",
                "version": "1.2.3",
                "masking": {"enabled": true, "replacement": "[REDACTED]", "preserve_last": 4},
                "sampling": {"enabled": true, "initial_rate": 10, "thereafter_rate": 2},
                "module_levels": {"sqlx": "warn", "rskit": "debug"},
                "otlp": {"enabled": true, "endpoint": "http://collector:4317", "protocol": "http", "insecure": true}
            }"#,
        )
        .unwrap();

        assert_eq!(cfg.level, "trace");
        assert_eq!(cfg.format, LogFormat::Text);
        assert_eq!(
            cfg.output,
            LogOutput::File {
                path: "app.log".to_string()
            }
        );
        assert!(cfg.no_color);
        assert!(!cfg.timestamp);
        assert!(cfg.with_caller);
        assert_eq!(cfg.environment.as_deref(), Some("production"));
        assert_eq!(cfg.version.as_deref(), Some("1.2.3"));
        assert_eq!(cfg.masking.preserve_last, 4);
        assert_eq!(cfg.masking.replacement, "[REDACTED]");
        assert!(cfg.sampling.enabled);
        assert_eq!(
            cfg.module_levels.get("sqlx").map(String::as_str),
            Some("warn")
        );
        assert!(cfg.otlp.enabled);
        assert!(cfg.otlp.insecure);
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
    fn log_output_deserialize_stdout_shorthand() {
        let out: LogOutput = serde_json::from_str(r#""stdout""#).unwrap();
        assert_eq!(out, LogOutput::Stdout);
    }

    #[test]
    fn log_output_deserialize_stderr() {
        let out: LogOutput = serde_json::from_str(r#"{"type":"stderr"}"#).unwrap();
        assert_eq!(out, LogOutput::Stderr);
    }

    #[test]
    fn log_output_deserialize_stderr_shorthand() {
        let out: LogOutput = serde_json::from_str(r#""stderr""#).unwrap();
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

    #[test]
    fn log_output_file_requires_path() {
        let err = serde_json::from_str::<LogOutput>(r#"{"type":"file"}"#).unwrap_err();
        assert!(err.to_string().contains("requires path"));
    }
}
