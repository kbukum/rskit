//! Structured logging setup using the [`tracing`] ecosystem.
//!
//! # Usage
//!
//! ```ignore
//! use rskit_logging::init_logging;
//!
//! let _guard = init_logging(&config.service.logging);
//! // _guard must stay alive for the duration of the program
//! tracing::info!(service = "my-svc", "started");
//! ```
//!
//! # Design
//!
//! There is intentionally no global logger registry (unlike gokit's `logger.Get(name)`).
//! Callers use `tracing` directly and scope context via spans + `#[tracing::instrument]`.
//! `init_logging` sets up the global subscriber once; the returned guard restores
//! the previous subscriber on drop (useful in tests).

use rskit_config::{LogFormat, LoggingConfig};
use tracing::dispatcher::DefaultGuard;
use tracing_subscriber::{fmt, layer::SubscriberExt, EnvFilter};

/// Opaque guard — drop to restore the previous tracing subscriber.
///
/// Keep this alive for the lifetime of your service (e.g. bind it to a
/// variable in `main`).
pub struct LoggingGuard(#[allow(dead_code)] DefaultGuard);

/// Initialize structured logging from a [`LoggingConfig`].
///
/// - `LogFormat::Json` → newline-delimited JSON (production)
/// - `LogFormat::Console` → human-readable with colour (development)
///
/// The `RUST_LOG` env var takes precedence over `cfg.level` when set.
pub fn init_logging(cfg: &LoggingConfig) -> LoggingGuard {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&cfg.level));

    let guard = match cfg.format {
        LogFormat::Json => {
            let layer = fmt::layer().json().with_current_span(true).with_span_list(true);
            let dispatcher = tracing_subscriber::registry()
                .with(filter)
                .with(layer)
                .into();
            tracing::dispatcher::set_default(&dispatcher)
        }
        LogFormat::Console => {
            let layer = fmt::layer().pretty();
            let dispatcher = tracing_subscriber::registry()
                .with(filter)
                .with(layer)
                .into();
            tracing::dispatcher::set_default(&dispatcher)
        }
    };

    LoggingGuard(guard)
}

/// Initialize logging from `RUST_LOG` only (no config struct needed).
pub fn init_logging_env() -> LoggingGuard {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));
    let layer = fmt::layer().pretty();
    let dispatcher = tracing_subscriber::registry()
        .with(filter)
        .with(layer)
        .into();
    LoggingGuard(tracing::dispatcher::set_default(&dispatcher))
}

// Re-export tracing macros for convenience — callers can `use rskit_logging::*`
pub use tracing::{debug, error, info, instrument, trace, warn};

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_config::LoggingConfig;

    #[test]
    fn init_console_does_not_panic() {
        let cfg = LoggingConfig::default();
        let _guard = init_logging(&cfg);
        tracing::info!("test log");
    }

    #[test]
    fn init_json_does_not_panic() {
        let cfg = LoggingConfig {
            format: rskit_config::LogFormat::Json,
            ..Default::default()
        };
        let _guard = init_logging(&cfg);
        tracing::info!("test json log");
    }
}
