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
use tracing_subscriber::{fmt, layer::SubscriberExt};

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
///
/// Does **not** enable masking.  Use [`init_global_with_masking`] or
/// [`init_global_with_options`] when masking is required.
pub fn init_global(cfg: &LoggingConfig) -> GlobalLoggingGuard {
    init_global_with_options(cfg, None, None, None)
}

/// Returns `true` if [`init_global`] has been called at least once.
pub fn is_global_init() -> bool {
    GLOBAL_INIT.load(Ordering::SeqCst)
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
    let m = if masking_cfg.enabled {
        Some(masking_cfg)
    } else {
        None
    };
    init_global_with_options(cfg, None, None, m)
}

/// Enhanced global init with optional sampling, per-module levels, and masking.
///
/// This is the primary global initialisation entry point.  All other
/// `init_global*` functions delegate here.
///
/// Same semantics as [`init_global`] (idempotent, global-lifetime subscriber)
/// but adds support for [`SamplingLayer`], per-module filter directives, and
/// output masking.
///
/// Pass `masking_cfg: None` to disable masking entirely.
pub fn init_global_with_options(
    cfg: &LoggingConfig,
    sampling_cfg: Option<&SamplingConfig>,
    module_levels: Option<&HashMap<String, String>>,
    masking_cfg: Option<&masking::MaskingConfig>,
) -> GlobalLoggingGuard {
    if GLOBAL_INIT
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return GlobalLoggingGuard { _private: () };
    }

    let filter = crate::build_filter(&cfg.level, module_levels);
    let base_writer = make_writer_for(&cfg.output);

    let sampling_layer = sampling_cfg.filter(|s| s.enabled).map(SamplingLayer::new);

    if let Some(m) = masking_cfg.filter(|m| m.enabled) {
        let masker: Arc<dyn masking::Masker> = Arc::new(masking::DefaultMasker::new(m));
        let writer = masking::MaskingMakeWriter::new(base_writer, masker);

        match cfg.format {
            LogFormat::Json => {
                let layer = fmt::layer()
                    .json()
                    .with_current_span(true)
                    .with_span_list(true)
                    .with_writer(writer);
                let subscriber = tracing_subscriber::registry()
                    .with(filter)
                    .with(sampling_layer)
                    .with(layer);
                if let Err(e) = tracing::subscriber::set_global_default(subscriber) {
                    tracing::warn!("global subscriber already installed, skipping: {e}");
                }
            }
            LogFormat::Console => {
                let layer = fmt::layer().pretty().with_writer(writer);
                let subscriber = tracing_subscriber::registry()
                    .with(filter)
                    .with(sampling_layer)
                    .with(layer);
                if let Err(e) = tracing::subscriber::set_global_default(subscriber) {
                    tracing::warn!("global subscriber already installed, skipping: {e}");
                }
            }
        }
    } else {
        match cfg.format {
            LogFormat::Json => {
                let layer = fmt::layer()
                    .json()
                    .with_current_span(true)
                    .with_span_list(true)
                    .with_writer(base_writer);
                let subscriber = tracing_subscriber::registry()
                    .with(filter)
                    .with(sampling_layer)
                    .with(layer);
                if let Err(e) = tracing::subscriber::set_global_default(subscriber) {
                    tracing::warn!("global subscriber already installed, skipping: {e}");
                }
            }
            LogFormat::Console => {
                let layer = fmt::layer().pretty().with_writer(base_writer);
                let subscriber = tracing_subscriber::registry()
                    .with(filter)
                    .with(sampling_layer)
                    .with(layer);
                if let Err(e) = tracing::subscriber::set_global_default(subscriber) {
                    tracing::warn!("global subscriber already installed, skipping: {e}");
                }
            }
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
