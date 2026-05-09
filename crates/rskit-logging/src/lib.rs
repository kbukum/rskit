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

#![warn(missing_docs)]

/// Span-level context helpers — component tagging, request enrichment.
pub mod context;
/// Standard field name constants for the unified log schema.
pub mod fields;
/// Global tracing subscriber initialisation.
pub mod global;
/// Sensitive data masking for log output.
pub mod masking;
/// Per-module log level overrides from config.
pub mod module_levels;
/// OpenTelemetry Logs bridge with OTLP export.
#[cfg(feature = "otlp")]
pub mod otlp;
/// Rate-based log sampling layer.
pub mod sampling;

use std::collections::HashMap;
use std::sync::Arc;

use rskit_config::{LogFormat, LoggingConfig};
use tracing::dispatcher::DefaultGuard;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt};

pub use global::{
    GlobalLoggingGuard, init_global, init_global_with_masking, init_global_with_options,
    is_global_init,
};
pub use masking::{DefaultMasker, Masker, MaskingConfig, MaskingMakeWriter};
pub use module_levels::{ModuleLevelsConfig, build_env_filter};
#[cfg(feature = "otlp")]
pub use otlp::{OtlpConfig, OtlpProvider};
pub use sampling::SamplingConfig;

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
///
/// # Security
///
/// Use [`init_logging_with_masking`] or [`crate::global::init_global_with_masking`]
/// when log output must be redacted before it reaches the configured sink.
pub fn init_logging(cfg: &LoggingConfig) -> LoggingGuard {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&cfg.level));

    let guard = match cfg.format {
        LogFormat::Json => {
            let layer = fmt::layer()
                .json()
                .with_current_span(true)
                .with_span_list(true);
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
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let layer = fmt::layer().pretty();
    let dispatcher = tracing_subscriber::registry()
        .with(filter)
        .with(layer)
        .into();
    LoggingGuard(tracing::dispatcher::set_default(&dispatcher))
}

/// Enhanced logging init with optional sampling and per-module level overrides.
///
/// This extends [`init_logging`] with two additional capabilities:
///
/// - **Sampling** — when `sampling` is `Some` and enabled, a [`sampling::SamplingLayer`]
///   is added to drop events exceeding per-level rate limits.
/// - **Module levels** — when `module_levels` is `Some`, per-module filter directives
///   are merged into the [`EnvFilter`].
///
/// The original [`init_logging`] function continues to work unchanged.
pub fn init_logging_with_options(
    cfg: &LoggingConfig,
    sampling_cfg: Option<&SamplingConfig>,
    module_levels: Option<&HashMap<String, String>>,
) -> LoggingGuard {
    let filter = build_filter(&cfg.level, module_levels);

    let sampling_layer = sampling_cfg
        .filter(|s| s.enabled)
        .map(sampling::SamplingLayer::new);

    let guard = match cfg.format {
        LogFormat::Json => {
            let layer = fmt::layer()
                .json()
                .with_current_span(true)
                .with_span_list(true);
            let dispatcher = tracing_subscriber::registry()
                .with(filter)
                .with(sampling_layer)
                .with(layer)
                .into();
            tracing::dispatcher::set_default(&dispatcher)
        }
        LogFormat::Console => {
            let layer = fmt::layer().pretty();
            let dispatcher = tracing_subscriber::registry()
                .with(filter)
                .with(sampling_layer)
                .with(layer)
                .into();
            tracing::dispatcher::set_default(&dispatcher)
        }
    };

    LoggingGuard(guard)
}

/// Initialize logging with sensitive data masking.
///
/// When `masking_cfg.enabled` is `true`, all log output passes through a
/// [`MaskingMakeWriter`] that redacts secrets and PII before they reach the
/// output sink.  When masking is disabled this delegates to [`init_logging`].
pub fn init_logging_with_masking(
    cfg: &LoggingConfig,
    masking_cfg: &masking::MaskingConfig,
) -> LoggingGuard {
    if !masking_cfg.enabled {
        return init_logging(cfg);
    }

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&cfg.level));
    let masker: Arc<dyn masking::Masker> = Arc::new(masking::DefaultMasker::new(masking_cfg));
    let writer = masking::MaskingMakeWriter::new(std::io::stdout, masker);

    let guard = match cfg.format {
        LogFormat::Json => {
            let layer = fmt::layer()
                .json()
                .with_current_span(true)
                .with_span_list(true)
                .with_writer(writer);
            let dispatcher = tracing_subscriber::registry()
                .with(filter)
                .with(layer)
                .into();
            tracing::dispatcher::set_default(&dispatcher)
        }
        LogFormat::Console => {
            let layer = fmt::layer().pretty().with_writer(writer);
            let dispatcher = tracing_subscriber::registry()
                .with(filter)
                .with(layer)
                .into();
            tracing::dispatcher::set_default(&dispatcher)
        }
    };

    LoggingGuard(guard)
}

/// Build an [`EnvFilter`] from the configured level and optional module overrides.
fn build_filter(level: &str, module_levels: Option<&HashMap<String, String>>) -> EnvFilter {
    match module_levels {
        Some(levels) if !levels.is_empty() => module_levels::build_env_filter(level, levels),
        _ => EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level)),
    }
}

/// Enhanced logging init with all options including OTLP export.
///
/// Layers the subscriber stack as follows:
///
/// 1. [`EnvFilter`] — base level + optional per-module overrides
/// 2. Optional [`sampling::SamplingLayer`] — rate-based log sampling
/// 3. Format layer (JSON or console)
/// 4. Optional [`otlp::OtlpProvider`] layer — bridges events to OTel Logs SDK
///
/// The returned [`LoggingGuard`] **must** be held for the lifetime of the
/// service.  When dropped it restores the previous subscriber and (when OTLP
/// is enabled) shuts down the provider, flushing pending logs.
///
/// # Errors
///
/// Returns an error if the OTLP provider cannot be created (e.g. invalid
/// endpoint or transport failure).
#[cfg(feature = "otlp")]
pub fn init_logging_full(
    cfg: &LoggingConfig,
    sampling_cfg: Option<&SamplingConfig>,
    module_levels: Option<&HashMap<String, String>>,
    otlp_cfg: Option<&otlp::OtlpConfig>,
    service_name: &str,
    environment: &str,
    version: &str,
) -> Result<LoggingGuard, Box<dyn std::error::Error + Send + Sync>> {
    let filter = build_filter(&cfg.level, module_levels);

    let sampling_layer = sampling_cfg
        .filter(|s| s.enabled)
        .map(sampling::SamplingLayer::new);

    let otlp_layer = match otlp_cfg {
        Some(oc) => otlp::OtlpProvider::new(oc, service_name, environment, version)?
            .map(|p| p.layer::<tracing_subscriber::Registry>()),
        None => None,
    };

    let guard = match cfg.format {
        LogFormat::Json => {
            let layer = fmt::layer()
                .json()
                .with_current_span(true)
                .with_span_list(true);
            let dispatcher = tracing_subscriber::registry()
                .with(filter)
                .with(sampling_layer)
                .with(layer)
                .with(otlp_layer)
                .into();
            tracing::dispatcher::set_default(&dispatcher)
        }
        LogFormat::Console => {
            let layer = fmt::layer().pretty();
            let dispatcher = tracing_subscriber::registry()
                .with(filter)
                .with(sampling_layer)
                .with(layer)
                .with(otlp_layer)
                .into();
            tracing::dispatcher::set_default(&dispatcher)
        }
    };

    Ok(LoggingGuard(guard))
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
