//! Structured logging setup using the [`tracing`](https://docs.rs/tracing) ecosystem.
//!
//! # Layers
//!
//! - **Vocabulary (always available)** —
//!   [`config::LoggingConfig`] / [`config::LogFormat`] / [`config::LogOutput`] describe logging declaratively
//!   and carry no `tracing` dependency,
//!   so configuration crates can compose them without linking the subscriber stack.
//! - **Setup (`setup` feature, default-on)** — `init_logging` and friends, the `LoggingGuard`, masking,
//!   sampling, per-module levels, and context helpers.
//!   Everything built on `tracing`/`tracing-subscriber` lives here.
//!
//! # Usage
//!
//! ```ignore
//! use rskit_logging::init_logging;
//!
//! let _guard = init_logging(&config.service.logging)?;
//! // _guard must stay alive for the duration of the program
//! tracing::info!(service = "my-svc", "started");
//! ```
//!
//! # Design
//!
//! There is intentionally no global logger registry.
//! Callers use `tracing` directly and scope context via spans + `#[tracing::instrument]`.
//! `init_logging` installs a scoped default subscriber;
//! the returned guard restores the previous subscriber on drop.

#![warn(missing_docs)]

/// Logging configuration vocabulary (tracing-free, always available).
pub mod config;
/// Error helpers for logging setup.
pub mod error;
/// Standard field name constants for the unified log schema.
pub mod fields;

/// Span-level context helpers — component tagging, request enrichment.
#[cfg(feature = "setup")]
pub mod context;
/// Sensitive data masking for log output.
#[cfg(feature = "setup")]
pub mod masking;
/// Per-module log level overrides from config.
#[cfg(feature = "setup")]
pub mod module_levels;
/// OpenTelemetry Logs bridge with OTLP export.
#[cfg(feature = "otlp")]
pub mod otlp;
/// Rate-based log sampling layer.
#[cfg(feature = "setup")]
pub mod sampling;
/// Subscriber setup — building and installing `tracing` subscribers.
#[cfg(feature = "setup")]
pub mod setup;

pub use config::{
    LogFormat, LogOutput, LoggingConfig, MaskingConfig, ModuleLevelsConfig, OtlpConfig,
    SamplingConfig,
};
pub use error::LoggingResult;

#[cfg(feature = "setup")]
pub use masking::{DefaultMasker, Masker, MaskingMakeWriter};
#[cfg(feature = "setup")]
pub use module_levels::build_env_filter;
#[cfg(feature = "otlp")]
pub use otlp::OtlpProvider;
#[cfg(feature = "setup")]
pub use setup::{
    LoggingGuard, init_logging, init_logging_env, init_logging_with_masking,
    init_logging_with_options,
};
#[cfg(feature = "otlp")]
pub use setup::{LoggingSetup, init_logging_full};

// Logging macros exposed by this crate.
#[cfg(feature = "setup")]
pub use tracing::{debug, error, info, instrument, trace, warn};
