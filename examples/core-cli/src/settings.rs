//! Typed application settings loaded from a single strict TOML file.

use rskit_errors::AppResult;
use rskit_logging::LoggingConfig;
use serde::Deserialize;

/// Top-level configuration for `core-cli`.
///
/// `deny_unknown_fields` makes the strict loader reject typos and stray keys
/// instead of silently ignoring them — the "strict config" consumer story.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    /// Human-readable application name.
    pub app_name: String,
    /// Number of work units the `run` command should process.
    pub workers: u32,
    /// Logging vocabulary (level, format, output) owned by `rskit-logging`.
    #[serde(default)]
    pub logging: LoggingConfig,
}

/// Load [`Settings`] from a single strict TOML file at `path`.
///
/// Uses `rskit-config`'s strict loader, so unknown keys are rejected rather
/// than silently ignored.
///
/// # Errors
///
/// Returns a typed [`rskit_errors::AppError`] when the file is missing,
/// malformed, or contains unknown fields.
pub fn load(path: &str) -> AppResult<Settings> {
    rskit_config::load_strict(path)
}
