//! CLI framework: theming, structured output, progress, prompts, and signals.
//!
//! A parser-agnostic toolkit for building consistent command-line UX: it owns
//! the terminal *presentation* concerns (color, glyphs, tables, status lines,
//! progress bars), *input* (interactive prompts with a non-interactive
//! fallback), and cooperative *cancellation*, so every rskit CLI renders and
//! behaves the same way.
//!
//! # Modules
//!
//! - [`theme`] — visual vocabulary: semantic [`Palette`] color and [`Glyphs`]
//!   symbols, both honouring `NO_COLOR`, TTY, and UTF-8 capability.
//! - [`render`] — structured, non-interactive display: [`OutputTable`],
//!   [`OutputKV`], the [`ErrorRenderer`]/[`ExitCode`] convention, and one-off
//!   [`StatusReporter`] feedback lines.
//! - [`progress`] — progress bar and spinner abstractions over `indicatif`.
//! - [`live`] — multi-region live terminal rendering ([`LiveConsole`]) for
//!   streaming several concurrent outputs as bounded tiles.
//! - [`prompt`] — interactive prompts (line, rich raw-mode, and scripted media)
//!   with a non-interactive fallback.
//! - [`signal`] — Ctrl+C / graceful shutdown via [`CancellationToken`].

pub mod live;
pub mod progress;
pub mod prompt;
pub mod render;
pub mod signal;
pub mod theme;

pub use live::{LiveConfig, LiveConsole, RegionScreen};
pub use progress::{MultiProgress, ProgressBar, ProgressStyle};
pub use prompt::{
    Capabilities, Choice, ChoiceId, Key, LineTerminal, PromptMode, Prompter, ScriptedTerminal,
    Terminal, Validation, Validator, non_empty,
};
pub use render::{ErrorRenderer, ExitCode, OutputFormat, OutputKV, OutputTable, StatusReporter};
pub use signal::{CancellationToken, on_ctrl_c};
pub use theme::{
    ColorChoice, Glyphs, NO_COLOR_ENV, Palette, resolve_color, resolve_color_with,
    unicode_env_enabled,
};

#[cfg(feature = "interactive")]
pub use prompt::RichTerminal;
