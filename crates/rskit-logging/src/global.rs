//! Global tracing subscriber initialisation.
//!
//! Unlike [`crate::init_logging`] (which sets a *default* subscriber scoped to
//! the returned guard), `init_global` installs a *global* subscriber that
//! persists for the lifetime of the process.  Use it in `main()` when you do
//! not need the guard pattern.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rskit_config::{LogFormat, LogOutput, LoggingConfig};
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt};

use crate::masking;
use crate::sampling::{SamplingConfig, SamplingLayer};

static GLOBAL_INIT: AtomicBool = AtomicBool::new(false);

/// Opaque guard returned by [`init_global`].
///
/// Unlike [`crate::LoggingGuard`] this does **not** restore a previous
/// subscriber on drop — the global subscriber persists until process exit.
pub struct GlobalLoggingGuard {
    _private: (),
}

/// Initialise a global default subscriber that all `tracing::` calls fall back
/// to when no local subscriber is set.
///
/// Safe to call multiple times from `main` — subsequent calls are no-ops and
/// the original subscriber is kept.
pub fn init_global(cfg: &LoggingConfig) -> GlobalLoggingGuard {
    if GLOBAL_INIT
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return GlobalLoggingGuard { _private: () };
    }

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&cfg.level));

    let make_writer = make_writer_for(&cfg.output);

    match cfg.format {
        LogFormat::Json => {
            let layer = fmt::layer()
                .json()
                .with_current_span(true)
                .with_span_list(true)
                .with_writer(make_writer);
            let subscriber = tracing_subscriber::registry().with(filter).with(layer);
            tracing::subscriber::set_global_default(subscriber)
                .expect("failed to install global tracing subscriber; another subscriber may already be set");
        }
        LogFormat::Console => {
            let layer = fmt::layer().pretty().with_writer(make_writer);
            let subscriber = tracing_subscriber::registry().with(filter).with(layer);
            tracing::subscriber::set_global_default(subscriber)
                .expect("failed to install global tracing subscriber; another subscriber may already be set");
        }
    }

    GlobalLoggingGuard { _private: () }
}

/// Returns `true` if [`init_global`] has been called at least once.
pub fn is_global_init() -> bool {
    GLOBAL_INIT.load(Ordering::SeqCst)
}

/// Enhanced global init with optional sampling and per-module level overrides.
///
/// Same semantics as [`init_global`] (idempotent, global-lifetime subscriber)
/// but adds support for [`SamplingLayer`] and per-module filter directives.
pub fn init_global_with_options(
    cfg: &LoggingConfig,
    sampling_cfg: Option<&SamplingConfig>,
    module_levels: Option<&HashMap<String, String>>,
) -> GlobalLoggingGuard {
    if GLOBAL_INIT
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return GlobalLoggingGuard { _private: () };
    }

    let filter = crate::build_filter(&cfg.level, module_levels);
    let make_writer = make_writer_for(&cfg.output);

    let sampling_layer = sampling_cfg.filter(|s| s.enabled).map(SamplingLayer::new);

    match cfg.format {
        LogFormat::Json => {
            let layer = fmt::layer()
                .json()
                .with_current_span(true)
                .with_span_list(true)
                .with_writer(make_writer);
            let subscriber = tracing_subscriber::registry()
                .with(filter)
                .with(sampling_layer)
                .with(layer);
            tracing::subscriber::set_global_default(subscriber)
                .expect("failed to install global tracing subscriber; another subscriber may already be set");
        }
        LogFormat::Console => {
            let layer = fmt::layer().pretty().with_writer(make_writer);
            let subscriber = tracing_subscriber::registry()
                .with(filter)
                .with(sampling_layer)
                .with(layer);
            tracing::subscriber::set_global_default(subscriber)
                .expect("failed to install global tracing subscriber; another subscriber may already be set");
        }
    }

    GlobalLoggingGuard { _private: () }
}

/// Initialise a global subscriber with sensitive data masking.
///
/// When `masking_cfg.enabled` is `true`, all log output passes through a
/// [`masking::MaskingMakeWriter`] that redacts secrets and PII.  When
/// masking is disabled this delegates to [`init_global`].
///
/// Like [`init_global`], this is idempotent — subsequent calls are no-ops.
pub fn init_global_with_masking(
    cfg: &LoggingConfig,
    masking_cfg: &masking::MaskingConfig,
) -> GlobalLoggingGuard {
    if !masking_cfg.enabled {
        return init_global(cfg);
    }

    if GLOBAL_INIT
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return GlobalLoggingGuard { _private: () };
    }

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&cfg.level));
    let masker: Arc<dyn masking::Masker> = Arc::new(masking::DefaultMasker::new(masking_cfg));
    let inner_writer = make_writer_for(&cfg.output);
    let writer = masking::MaskingMakeWriter::new(inner_writer, masker);

    match cfg.format {
        LogFormat::Json => {
            let layer = fmt::layer()
                .json()
                .with_current_span(true)
                .with_span_list(true)
                .with_writer(writer);
            let subscriber = tracing_subscriber::registry().with(filter).with(layer);
            tracing::subscriber::set_global_default(subscriber)
                .expect("failed to install global tracing subscriber; another subscriber may already be set");
        }
        LogFormat::Console => {
            let layer = fmt::layer().pretty().with_writer(writer);
            let subscriber = tracing_subscriber::registry().with(filter).with(layer);
            tracing::subscriber::set_global_default(subscriber)
                .expect("failed to install global tracing subscriber; another subscriber may already be set");
        }
    }

    GlobalLoggingGuard { _private: () }
}

// ── Internal helpers ─────────────────────────────────────────────────────────

fn make_writer_for(output: &LogOutput) -> WriterKind {
    match output {
        LogOutput::Stderr => WriterKind::Stderr,
        // File output not yet supported — fall back to stdout
        LogOutput::File { .. } => WriterKind::Stdout,
        _ => WriterKind::Stdout,
    }
}

#[derive(Clone)]
enum WriterKind {
    Stdout,
    Stderr,
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for WriterKind {
    type Writer = Box<dyn std::io::Write>;

    fn make_writer(&'a self) -> Self::Writer {
        match self {
            WriterKind::Stdout => Box::new(std::io::stdout()),
            WriterKind::Stderr => Box::new(std::io::stderr()),
        }
    }
}
