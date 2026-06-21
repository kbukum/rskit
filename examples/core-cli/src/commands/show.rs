//! `show` subcommand: load config, initialise logging, render settings.

use rskit_cli::OutputKV;
use rskit_errors::AppResult;
use rskit_logging::{LoggingGuard, info, init_logging};

use crate::settings::{self, Settings};

/// Load settings from `path`, initialise logging, and return a rendered view.
///
/// The returned [`LoggingGuard`] must be held for the lifetime of the program
/// so buffered log output is flushed on drop.
///
/// # Errors
///
/// Propagates any [`rskit_errors::AppError`] from config loading or logging
/// initialisation.
pub fn execute(path: &str) -> AppResult<(LoggingGuard, String)> {
    let settings = settings::load(path)?;
    let guard = init_logging(&settings.logging)?;
    info!(app = %settings.app_name, "loaded configuration");
    Ok((guard, render(&settings)))
}

/// Render [`Settings`] as a key-value block.
///
/// Pure and side-effect free, so tests can assert on it without installing a
/// global logging subscriber.
#[must_use]
pub fn render(settings: &Settings) -> String {
    let mut kv = OutputKV::new();
    kv.add("app_name", settings.app_name.as_str())
        .add("workers", settings.workers.to_string())
        .add("log_level", settings.logging.level.as_str())
        .add("log_format", format!("{:?}", settings.logging.format));
    kv.to_string()
}
