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

/// Initialize structured logging from a [`LoggingConfig`] with default masking.
///
/// - `LogFormat::Json` → newline-delimited JSON (production)
/// - `LogFormat::Console` → human-readable with colour (development)
///
/// Masking is **enabled by default** — sensitive fields are redacted before
/// reaching the output sink.  Use [`init_logging_with_options`] for full
/// control over masking, sampling, and per-module levels.
///
/// The `RUST_LOG` env var takes precedence over `cfg.level` when set.
pub fn init_logging(cfg: &LoggingConfig) -> LoggingGuard {
    let default_masking = masking::MaskingConfig::default();
    init_logging_with_options(cfg, None, None, Some(&default_masking))
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

/// Initialize logging with explicit masking configuration.
///
/// When `masking_cfg.enabled` is `true`, all log output passes through a
/// [`MaskingMakeWriter`] that redacts secrets and PII before they reach the
/// output sink.  When masking is disabled, logging goes directly to stdout.
pub fn init_logging_with_masking(
    cfg: &LoggingConfig,
    masking_cfg: &masking::MaskingConfig,
) -> LoggingGuard {
    let m = if masking_cfg.enabled {
        Some(masking_cfg)
    } else {
        None
    };
    init_logging_with_options(cfg, None, None, m)
}

/// Enhanced logging init with optional sampling, per-module levels, and masking.
///
/// This is the primary initialisation entry point.  All other `init_logging*`
/// functions delegate here.
///
/// - **Sampling** — when `sampling` is `Some` and enabled, a [`sampling::SamplingLayer`]
///   is added to drop events exceeding per-level rate limits.
/// - **Module levels** — when `module_levels` is `Some`, per-module filter directives
///   are merged into the [`EnvFilter`].
/// - **Masking** — when `masking_cfg` is `Some` and enabled, a [`MaskingMakeWriter`]
///   wraps stdout to redact secrets.  Pass `None` to disable masking entirely.
pub fn init_logging_with_options(
    cfg: &LoggingConfig,
    sampling_cfg: Option<&SamplingConfig>,
    module_levels: Option<&HashMap<String, String>>,
    masking_cfg: Option<&MaskingConfig>,
) -> LoggingGuard {
    let filter = build_filter(&cfg.level, module_levels);

    let sampling_layer = sampling_cfg
        .filter(|s| s.enabled)
        .map(sampling::SamplingLayer::new);

    if let Some(m) = masking_cfg.filter(|m| m.enabled) {
        let masker: Arc<dyn masking::Masker> = Arc::new(masking::DefaultMasker::new(m));
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
                    .with(sampling_layer)
                    .with(layer)
                    .into();
                tracing::dispatcher::set_default(&dispatcher)
            }
            LogFormat::Console => {
                let layer = fmt::layer().pretty().with_writer(writer);
                let dispatcher = tracing_subscriber::registry()
                    .with(filter)
                    .with(sampling_layer)
                    .with(layer)
                    .into();
                tracing::dispatcher::set_default(&dispatcher)
            }
        };
        return LoggingGuard(guard);
    }

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
    masking_cfg: Option<&MaskingConfig>,
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

    if let Some(m) = masking_cfg.filter(|m| m.enabled) {
        let masker: Arc<dyn masking::Masker> = Arc::new(masking::DefaultMasker::new(m));
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
                    .with(sampling_layer)
                    .with(layer)
                    .with(otlp_layer)
                    .into();
                tracing::dispatcher::set_default(&dispatcher)
            }
            LogFormat::Console => {
                let layer = fmt::layer().pretty().with_writer(writer);
                let dispatcher = tracing_subscriber::registry()
                    .with(filter)
                    .with(sampling_layer)
                    .with(layer)
                    .with(otlp_layer)
                    .into();
                tracing::dispatcher::set_default(&dispatcher)
            }
        };
        return Ok(LoggingGuard(guard));
    }

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

// Logging macros exposed by this crate.
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
