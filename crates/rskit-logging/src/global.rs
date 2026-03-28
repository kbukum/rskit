//! Global tracing subscriber initialisation.
//!
//! Unlike [`crate::init_logging`] (which sets a *default* subscriber scoped to
//! the returned guard), `init_global` installs a *global* subscriber that
//! persists for the lifetime of the process.  Use it in `main()` when you do
//! not need the guard pattern.

use std::sync::atomic::{AtomicBool, Ordering};

use rskit_config::{LogFormat, LogOutput, LoggingConfig};
use tracing_subscriber::{fmt, layer::SubscriberExt, EnvFilter};

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

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&cfg.level));

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
                .expect("global subscriber already set");
        }
        LogFormat::Console => {
            let layer = fmt::layer().pretty().with_writer(make_writer);
            let subscriber = tracing_subscriber::registry().with(filter).with(layer);
            tracing::subscriber::set_global_default(subscriber)
                .expect("global subscriber already set");
        }
    }

    GlobalLoggingGuard { _private: () }
}

/// Returns `true` if [`init_global`] has been called at least once.
pub fn is_global_init() -> bool {
    GLOBAL_INIT.load(Ordering::SeqCst)
}

// ── Internal helpers ─────────────────────────────────────────────────────────

fn make_writer_for(output: &LogOutput) -> impl tracing_subscriber::fmt::MakeWriter<'static> + Clone {
    // We only support Stdout/Stderr directly; File requires boxing and is
    // deferred to a non-Send context — use Stdout as safe fallback for now.
    match output {
        LogOutput::Stderr => WriterKind::Stderr,
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
