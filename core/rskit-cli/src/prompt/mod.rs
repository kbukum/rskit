//! Interactive prompts for guided CLI flows.
//!
//! A cohesive input layer beside [`crate::render`] and [`crate::theme`]:
//! it asks a question plus a fixed set of [`Choice`]s and returns a typed answer,
//! reusing the resolved [`Palette`](crate::theme::Palette) and [`Glyphs`](crate::theme::Glyphs)
//! so styling honours `NO_COLOR`, TTY, and UTF-8 detection exactly like the rest of the CLI layer.
//!
//! Prompts speak through a [`Terminal`] seam rather than raw stdio,
//! so the same question renders as a numbered list over a pipe, as a live arrow-key widget on a TTY,
//! or as a scripted sequence in a test — without the calling code changing.
//!
//! # Modules
//!
//! - [`choice`] — [`Choice`] and its stable [`ChoiceId`].
//! - [`mode`] — [`PromptMode`] interactive/non-interactive resolution.
//! - [`key`] — the [`Key`] keystroke vocabulary for key-driven terminals.
//! - [`validate`] — the [`Validator`] trait for re-asking on invalid input.
//! - [`terminal`] — the [`Terminal`] seam: line, rich (raw-mode), and scripted media.
//! - [`render`] — shared choice/frame rendering used by every terminal.
//! - [`kinds`] — one behavioural module per question type.
//! - [`prompter`] — the [`Prompter`] driver and its question methods.
//!
//! # Interaction models
//!
//! One set of prompt-kind logic drives three realities, chosen automatically:
//!
//! - a **line** terminal (cooked stdio) prints a numbered list and reads a typed answer —
//!   always available, dependency-free, and pipe-friendly;
//! - a **rich** terminal (raw mode, behind the `interactive` feature) reads arrow keys
//!   and space to drive live radio and checkbox lists;
//! - a **scripted** terminal replays canned keys or lines for deterministic tests.
//!
//! [`Prompter::from_env`] selects rich when a TTY is present and the feature is compiled, else line;
//! tests bind a [`ScriptedTerminal`] directly.
//!
//! # Non-interactive fallback
//!
//! The driver resolves its behaviour once, up front, from a [`PromptMode`]:
//!
//! - [`PromptMode::Interactive`] — render prompts and read answers.
//! - [`PromptMode::NonInteractive`] — never block:
//!   each question resolves to its declared default (the `recommended` [`Choice`] for a selection, the supplied default for a confirm/text).
//!   A required question with no default is a typed [`AppError`](rskit_errors::AppError) rather than an invented answer
//!   or a hang.

pub mod choice;
pub mod key;
pub mod kinds;
pub mod mode;
pub mod prompter;
pub mod render;
pub mod terminal;
pub mod validate;

pub use choice::{Choice, ChoiceId};
pub use key::Key;
pub use mode::PromptMode;
pub use prompter::Prompter;
pub use terminal::{Capabilities, LineTerminal, ScriptedTerminal, Terminal};
pub use validate::{Validation, Validator, non_empty};

#[cfg(feature = "interactive")]
pub use terminal::RichTerminal;
