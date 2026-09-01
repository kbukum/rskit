//! Subscriber setup — building and installing `tracing` subscribers.
//!
//! This module owns everything that depends on the `tracing` ecosystem: [`init_logging`] and friends,
//! the [`LoggingGuard`], and the `EnvFilter`/output plumbing.
//! It is gated behind the default-on `setup` feature
//! so that consumers wanting only the configuration vocabulary (see [`crate::config`]) do not link `tracing-subscriber`.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::sync::Arc;

use tracing::Subscriber;
use tracing::dispatcher::DefaultGuard;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{EnvFilter, Layer, fmt, fmt::writer::BoxMakeWriter, layer::SubscriberExt};

use crate::config::{LogFormat, LogOutput, LoggingConfig};
use crate::error::{self, LoggingResult};
use crate::masking::{self, MaskingConfig};
use crate::module_levels;
use crate::sampling::{self, SamplingConfig};

/// Options for [`init_logging_full`].
#[cfg(feature = "otlp")]
pub struct LoggingSetup<'a> {
    /// Base logging configuration.
    pub config: &'a LoggingConfig,
    /// Optional rate-based sampling configuration.
    pub sampling: Option<&'a SamplingConfig>,
    /// Optional per-module level overrides.
    pub module_levels: Option<&'a HashMap<String, String>>,
    /// Optional sensitive data masking configuration.
    pub masking: Option<&'a MaskingConfig>,
    /// Optional OTLP exporter configuration.
    pub otlp: Option<&'a crate::otlp::OtlpConfig>,
    /// Service name reported to OpenTelemetry.
    pub service_name: &'a str,
    /// Deployment environment reported to OpenTelemetry.
    pub environment: &'a str,
    /// Service version reported to OpenTelemetry.
    pub version: &'a str,
}

#[cfg(feature = "otlp")]
impl<'a> LoggingSetup<'a> {
    /// Create full logging setup options with no optional layers enabled.
    #[must_use]
    pub const fn new(
        config: &'a LoggingConfig,
        service_name: &'a str,
        environment: &'a str,
        version: &'a str,
    ) -> Self {
        Self {
            config,
            sampling: None,
            module_levels: None,
            masking: None,
            otlp: None,
            service_name,
            environment,
            version,
        }
    }

    /// Add rate-based sampling.
    #[must_use]
    pub const fn with_sampling(mut self, sampling: &'a SamplingConfig) -> Self {
        self.sampling = Some(sampling);
        self
    }

    /// Add per-module log level overrides.
    #[must_use]
    pub const fn with_module_levels(mut self, module_levels: &'a HashMap<String, String>) -> Self {
        self.module_levels = Some(module_levels);
        self
    }

    /// Add sensitive data masking.
    #[must_use]
    pub const fn with_masking(mut self, masking: &'a MaskingConfig) -> Self {
        self.masking = Some(masking);
        self
    }

    /// Add OTLP export.
    #[must_use]
    pub const fn with_otlp(mut self, otlp: &'a crate::otlp::OtlpConfig) -> Self {
        self.otlp = Some(otlp);
        self
    }
}

/// Opaque guard — drop to restore the previous tracing subscriber.
///
/// Keep this alive for the lifetime of your service (e.g. bind it to a variable in `main`).
/// When OTLP export is enabled through `init_logging_full`, the guard also owns the OTLP provider
/// and shuts it down on drop to flush pending records.
pub struct LoggingGuard {
    #[allow(dead_code)]
    guard: DefaultGuard,
    #[cfg(feature = "otlp")]
    otlp_provider: Option<crate::otlp::OtlpProvider>,
}

impl LoggingGuard {
    fn new(guard: DefaultGuard) -> Self {
        Self {
            guard,
            #[cfg(feature = "otlp")]
            otlp_provider: None,
        }
    }

    #[cfg(feature = "otlp")]
    fn with_otlp_provider(
        guard: DefaultGuard,
        otlp_provider: Option<crate::otlp::OtlpProvider>,
    ) -> Self {
        Self {
            guard,
            otlp_provider,
        }
    }
}

#[cfg(feature = "otlp")]
impl Drop for LoggingGuard {
    fn drop(&mut self) {
        if let Some(provider) = self.otlp_provider.take()
            && let Err(error) = provider.shutdown()
        {
            tracing::warn!(%error, "failed to shut down OTLP logging provider");
        }
    }
}

/// Initialize structured logging from a [`LoggingConfig`] with default masking.
///
/// - `LogFormat::Json` → newline-delimited JSON (production)
/// - `LogFormat::Console` → human-readable with colour (development)
///
/// Masking is **enabled by default** — sensitive fields are redacted before reaching the output sink.
/// Use [`init_logging_with_options`] for full control over masking, sampling, and per-module levels.
///
/// The `RUST_LOG` env var takes precedence over `cfg.level` when set.
///
/// # Errors
///
/// Returns an error when the configured file output cannot be opened.
pub fn init_logging(cfg: &LoggingConfig) -> LoggingResult<LoggingGuard> {
    init_logging_with_default_masking(cfg)
}

/// Initialize logging from `RUST_LOG` only (no config struct needed).
pub fn init_logging_env() -> LoggingGuard {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let layer = fmt::layer().pretty();
    let dispatcher = tracing_subscriber::registry()
        .with(filter)
        .with(layer)
        .into();
    LoggingGuard::new(tracing::dispatcher::set_default(&dispatcher))
}

fn init_logging_with_default_masking(cfg: &LoggingConfig) -> LoggingResult<LoggingGuard> {
    let filter = build_filter(&cfg.level, Some(&cfg.module_levels));
    let masker: Arc<dyn masking::Masker> = Arc::new(masking::DefaultMasker::new(&cfg.masking)?);
    let writer = masking::MaskingMakeWriter::new(build_output_writer(&cfg.output)?, masker);

    let layer = build_format_layer(
        &cfg.format,
        cfg.no_color,
        cfg.with_caller,
        cfg.timestamp,
        writer,
    );
    let dispatcher = tracing_subscriber::registry()
        .with(filter)
        .with(layer)
        .into();

    Ok(LoggingGuard::new(tracing::dispatcher::set_default(
        &dispatcher,
    )))
}

/// Initialize logging with explicit masking configuration.
///
/// When `masking_cfg.enabled` is `true`,
/// all log output passes through a [`MaskingMakeWriter`](crate::masking::MaskingMakeWriter) that redacts secrets
/// and PII before they reach the output sink. When masking is disabled,
/// logging goes directly to the configured output.
pub fn init_logging_with_masking(
    cfg: &LoggingConfig,
    masking_cfg: &masking::MaskingConfig,
) -> LoggingResult<LoggingGuard> {
    let m = if masking_cfg.enabled {
        Some(masking_cfg)
    } else {
        None
    };
    init_logging_with_options(cfg, None, None, m)
}

/// Enhanced logging init with optional sampling, per-module levels, and masking.
///
/// This is the primary initialisation entry point.  All other `init_logging*` functions delegate here.
///
/// - **Sampling** — when `sampling` is `Some` and enabled,
///   a [`SamplingLayer`](crate::sampling::SamplingLayer) is added to drop events exceeding per-level rate limits.
/// - **Module levels** — when `module_levels` is `Some`,
///   per-module filter directives are merged into the [`EnvFilter`].
/// - **Masking** — when `masking_cfg` is `Some` and enabled,
///   a [`MaskingMakeWriter`](crate::masking::MaskingMakeWriter) wraps the configured output to redact secrets.
///   Pass `None` to disable masking entirely.
///
/// # Errors
///
/// Returns an error when a custom masking regex pattern is invalid
/// or when the configured file output cannot be opened.
pub fn init_logging_with_options(
    cfg: &LoggingConfig,
    sampling_cfg: Option<&SamplingConfig>,
    module_levels: Option<&HashMap<String, String>>,
    masking_cfg: Option<&MaskingConfig>,
) -> LoggingResult<LoggingGuard> {
    let filter = build_filter(&cfg.level, module_levels.or(Some(&cfg.module_levels)));

    let sampling_layer = sampling_cfg
        .or(Some(&cfg.sampling))
        .filter(|s| s.enabled)
        .map(sampling::SamplingLayer::new);
    let writer = build_output_writer(&cfg.output)?;

    if let Some(m) = masking_cfg.filter(|m| m.enabled) {
        let masker: Arc<dyn masking::Masker> = Arc::new(masking::DefaultMasker::new(m)?);
        let writer = masking::MaskingMakeWriter::new(writer, masker);
        let layer = build_format_layer(
            &cfg.format,
            cfg.no_color,
            cfg.with_caller,
            cfg.timestamp,
            writer,
        );
        let dispatcher = tracing_subscriber::registry()
            .with(filter)
            .with(sampling_layer)
            .with(layer)
            .into();
        return Ok(LoggingGuard::new(tracing::dispatcher::set_default(
            &dispatcher,
        )));
    }

    let layer = build_format_layer(
        &cfg.format,
        cfg.no_color,
        cfg.with_caller,
        cfg.timestamp,
        writer,
    );
    let dispatcher = tracing_subscriber::registry()
        .with(filter)
        .with(sampling_layer)
        .with(layer)
        .into();

    Ok(LoggingGuard::new(tracing::dispatcher::set_default(
        &dispatcher,
    )))
}

/// Build an [`EnvFilter`] from the configured level and optional module overrides.
fn build_filter(level: &str, module_levels: Option<&HashMap<String, String>>) -> EnvFilter {
    match module_levels {
        Some(levels) if !levels.is_empty() => module_levels::build_env_filter(level, levels),
        _ => EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level)),
    }
}

fn build_output_writer(output: &LogOutput) -> LoggingResult<BoxMakeWriter> {
    let writer = match output {
        LogOutput::Stdout => BoxMakeWriter::new(std::io::stdout),
        LogOutput::Stderr => BoxMakeWriter::new(std::io::stderr),
        LogOutput::File { path } => {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(|err| error::log_file_open(path.clone(), err))?;
            BoxMakeWriter::new(Arc::new(file))
        }
    };

    Ok(writer)
}

/// Build the format layer for the configured [`LogFormat`], honouring caller location and the
/// `timestamp` toggle.
///
/// The format families are kept distinct: `Json` emits newline-delimited JSON, `Text` emits
/// compact single-line output, and `Console`/`Pretty` emit the expanded human-readable format.
/// When `timestamp` is `false` the timestamp is suppressed for every family. The layer is boxed so
/// all families and both timestamp variants share one return type.
fn build_format_layer<S, W>(
    format: &LogFormat,
    no_color: bool,
    with_caller: bool,
    timestamp: bool,
    writer: W,
) -> Box<dyn Layer<S> + Send + Sync>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    W: for<'w> fmt::MakeWriter<'w> + Send + Sync + 'static,
{
    match format {
        LogFormat::Json => {
            let layer = fmt::layer()
                .json()
                .with_current_span(true)
                .with_span_list(true)
                .with_file(with_caller)
                .with_line_number(with_caller)
                .with_writer(writer);
            if timestamp {
                layer.boxed()
            } else {
                layer.without_time().boxed()
            }
        }
        LogFormat::Text => {
            let layer = fmt::layer()
                .compact()
                .with_ansi(!no_color)
                .with_file(with_caller)
                .with_line_number(with_caller)
                .with_writer(writer);
            if timestamp {
                layer.boxed()
            } else {
                layer.without_time().boxed()
            }
        }
        // `Console` and `Pretty` share the expanded human-readable format.
        _ => {
            let layer = fmt::layer()
                .pretty()
                .with_ansi(!no_color)
                .with_file(with_caller)
                .with_line_number(with_caller)
                .with_writer(writer);
            if timestamp {
                layer.boxed()
            } else {
                layer.without_time().boxed()
            }
        }
    }
}

/// Resolve an OTLP resource identity value, preferring the config override when present.
///
/// Returns the config-supplied value when `Some`, otherwise the base `LoggingSetup` value.
#[cfg(feature = "otlp")]
fn resolve_identity<'a>(config_value: Option<&'a str>, base: &'a str) -> &'a str {
    config_value.unwrap_or(base)
}

/// Enhanced logging init with all options including OTLP export.
///
/// Layers the subscriber stack as follows:
///
/// 1. [`EnvFilter`] — base level + per-module overrides
/// 2. Optional [`SamplingLayer`](crate::sampling::SamplingLayer) — rate-based log sampling
/// 3. Format layer (JSON, text, or console/pretty)
/// 4. Optional [`OtlpProvider`](crate::otlp::OtlpProvider) layer — bridges events to OTel Logs SDK
///
/// Every optional layer falls back to the policy embedded in [`LoggingSetup::config`] when the
/// corresponding explicit override is `None`: module levels, sampling, masking, and OTLP are all
/// taken from `config` unless a caller supplies an override. In particular the default-enabled
/// masking from `config.masking` is applied on this path without callers having to duplicate it.
///
/// The OTLP resource identity (`service.name`, `deployment.environment`, `service.version`) is taken
/// from [`LoggingConfig::service_name`], [`LoggingConfig::environment`], and
/// [`LoggingConfig::version`] when set, overriding the base [`LoggingSetup`] identity; otherwise the
/// base identity is used.
///
/// The returned [`LoggingGuard`] **must** be held for the lifetime of the service.
/// When dropped it restores the previous subscriber and (when OTLP is enabled) shuts down the provider,
/// flushing pending logs.
///
/// # Errors
///
/// Returns an error if the OTLP provider cannot be created (e.g. invalid endpoint or transport failure),
/// or if a custom masking regex pattern is invalid.
#[cfg(feature = "otlp")]
pub fn init_logging_full(setup: LoggingSetup<'_>) -> LoggingResult<LoggingGuard> {
    use crate::otlp;
    let filter = build_filter(
        &setup.config.level,
        setup.module_levels.or(Some(&setup.config.module_levels)),
    );

    let sampling_layer = setup
        .sampling
        .or(Some(&setup.config.sampling))
        .filter(|s| s.enabled)
        .map(sampling::SamplingLayer::new);
    let writer = build_output_writer(&setup.config.output)?;

    let otlp_cfg = setup.otlp.or(Some(&setup.config.otlp));
    // Config-supplied identity overrides the base `LoggingSetup` values for the exported OTLP
    // resource, so a cross-kit config that sets these keys is honored instead of being inert.
    let service_name = resolve_identity(setup.config.service_name.as_deref(), setup.service_name);
    let environment = resolve_identity(setup.config.environment.as_deref(), setup.environment);
    let version = resolve_identity(setup.config.version.as_deref(), setup.version);
    let otlp_provider = match otlp_cfg {
        Some(oc) => otlp::OtlpProvider::new(oc, service_name, environment, version)?,
        None => None,
    };
    let otlp_layer = otlp_provider
        .as_ref()
        .map(|p| p.layer::<tracing_subscriber::Registry>());

    let masking = setup.masking.or(Some(&setup.config.masking));
    if let Some(m) = masking.filter(|m| m.enabled) {
        let masker: Arc<dyn masking::Masker> = Arc::new(masking::DefaultMasker::new(m)?);
        let writer = masking::MaskingMakeWriter::new(writer, masker);
        let layer = build_format_layer(
            &setup.config.format,
            setup.config.no_color,
            setup.config.with_caller,
            setup.config.timestamp,
            writer,
        );
        let dispatcher = tracing_subscriber::registry()
            .with(filter)
            .with(sampling_layer)
            .with(layer)
            .with(otlp_layer)
            .into();
        return Ok(LoggingGuard::with_otlp_provider(
            tracing::dispatcher::set_default(&dispatcher),
            otlp_provider,
        ));
    }

    let layer = build_format_layer(
        &setup.config.format,
        setup.config.no_color,
        setup.config.with_caller,
        setup.config.timestamp,
        writer,
    );
    let dispatcher = tracing_subscriber::registry()
        .with(filter)
        .with(sampling_layer)
        .with(layer)
        .with(otlp_layer)
        .into();

    Ok(LoggingGuard::with_otlp_provider(
        tracing::dispatcher::set_default(&dispatcher),
        otlp_provider,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "otlp")]
    #[test]
    fn resolve_identity_prefers_config_override() {
        assert_eq!(resolve_identity(Some("cfg"), "base"), "cfg");
        assert_eq!(resolve_identity(None, "base"), "base");
    }

    #[test]
    fn init_console_does_not_panic() {
        let cfg = LoggingConfig::default();
        let _guard = init_logging(&cfg).unwrap();
        tracing::info!("test log");
    }

    #[test]
    fn init_json_does_not_panic() {
        let cfg = LoggingConfig {
            format: LogFormat::Json,
            ..Default::default()
        };
        let _guard = init_logging(&cfg).unwrap();
        tracing::info!("test json log");
    }

    fn capture_output(cfg: LoggingConfig) -> String {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.log");
        let cfg = LoggingConfig {
            output: LogOutput::File {
                path: path.to_string_lossy().into_owned(),
            },
            no_color: true,
            ..cfg
        };
        {
            let _guard = init_logging(&cfg).unwrap();
            tracing::info!("captured message");
        }
        std::fs::read_to_string(&path).unwrap()
    }

    #[test]
    fn json_timestamp_toggle_controls_output() {
        let with_ts = capture_output(LoggingConfig {
            format: LogFormat::Json,
            timestamp: true,
            ..Default::default()
        });
        assert!(
            with_ts.contains("\"timestamp\""),
            "expected a timestamp field, got: {with_ts}"
        );

        let without_ts = capture_output(LoggingConfig {
            format: LogFormat::Json,
            timestamp: false,
            ..Default::default()
        });
        assert!(
            !without_ts.contains("\"timestamp\""),
            "expected no timestamp field, got: {without_ts}"
        );
    }

    #[test]
    fn text_format_is_distinct_from_pretty() {
        // `Text` is a compact single-line format; `Pretty` expands across multiple lines.
        // This guards against `Text` being routed through the `Pretty` wildcard branch.
        let text = capture_output(LoggingConfig {
            format: LogFormat::Text,
            ..Default::default()
        });
        let pretty = capture_output(LoggingConfig {
            format: LogFormat::Pretty,
            ..Default::default()
        });

        assert_eq!(
            text.lines().filter(|l| !l.is_empty()).count(),
            1,
            "text output should be a single line, got: {text}"
        );
        assert!(
            pretty.lines().count() > text.lines().count(),
            "pretty output should span more lines than text; text={text:?} pretty={pretty:?}"
        );
    }
}
