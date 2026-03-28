//! CLI framework: progress bars, structured output, signal handling.
//!
//! Provides terminal progress bars (over indicatif), structured output
//! formatting, and Ctrl+C cancellation via [`CancellationToken`].
//!
//! # Modules
//!
//! - [`signal`] — Ctrl+C / graceful shutdown via `CancellationToken`
//! - [`progress`] — Progress bar abstractions over `indicatif`
//! - [`output`] — Structured terminal output (tables, key-value)

pub mod output;
pub mod progress;
pub mod signal;

pub use output::{OutputKV, OutputTable};
pub use progress::{MultiProgress, ProgressBar, ProgressStyle};
pub use signal::CancellationToken;
