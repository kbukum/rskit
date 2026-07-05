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
//! - [`color`] — Semantic terminal color with `NO_COLOR`/TTY resolution

pub mod color;
pub mod output;
pub mod progress;
pub mod signal;

pub use color::{ColorChoice, NO_COLOR_ENV, Palette, resolve_color, resolve_color_with};
pub use output::{ErrorRenderer, ExitCode, OutputFormat, OutputKV, OutputTable};
pub use progress::{MultiProgress, ProgressBar, ProgressStyle};
pub use signal::{CancellationToken, on_ctrl_c};
