//! The interactive medium a [`Prompter`](crate::prompt::Prompter) speaks through.
//!
//! A `Terminal` abstracts *how* a prompt reads input and renders — decoupled
//! from *what* a prompt asks — so one set of prompt-kind logic drives three very
//! different realities:
//!
//! - [`mod@line`] — [`LineTerminal`]: plain cooked stdio (type a line, press Enter);
//!   no raw mode, works over pipes, dependency-free. The always-available default.
//! - `rich` — `RichTerminal`: a raw-mode terminal (behind the `interactive`
//!   feature) that reads individual [`Key`]s so widgets can offer arrow-key
//!   navigation with live-highlighted radio and checkbox lists.
//! - [`mod@scripted`] — [`ScriptedTerminal`]: a deterministic test double that feeds
//!   canned keys or lines and captures rendered output, so both the key-driven
//!   and line-driven paths are unit-testable without a real terminal.
//!
//! A terminal advertises whether it is key-driven via [`Capabilities`]; prompt
//! kinds branch on that once and never touch a concrete terminal type.

pub mod line;
pub mod scripted;

#[cfg(feature = "interactive")]
pub mod rich;

use rskit_errors::AppResult;

use super::key::Key;

pub use line::LineTerminal;
pub use scripted::ScriptedTerminal;

#[cfg(feature = "interactive")]
pub use rich::RichTerminal;

/// What interaction model a [`Terminal`] supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    keys: bool,
}

impl Default for Capabilities {
    /// Line-driven, matching the default (cooked-stdio) terminal.
    fn default() -> Self {
        Self::line_driven()
    }
}

impl Capabilities {
    /// A key-driven terminal: reads individual [`Key`]s and can redraw frames,
    /// enabling live arrow-key navigation.
    #[must_use]
    pub const fn key_driven() -> Self {
        Self { keys: true }
    }

    /// A line-driven terminal: reads a whole line at a time (cooked stdio).
    #[must_use]
    pub const fn line_driven() -> Self {
        Self { keys: false }
    }

    /// Whether the terminal reads individual keys (live widgets) rather than
    /// whole lines.
    #[must_use]
    pub const fn is_key_driven(self) -> bool {
        self.keys
    }
}

/// The interactive medium a prompter reads from and renders to.
///
/// Implementations own the I/O device and, for key-driven terminals, the
/// raw-mode lifecycle and cursor movement needed to redraw a live frame. The
/// prompter builds already-styled strings; the terminal only writes them and,
/// between frames, clears the lines it previously drew.
pub trait Terminal {
    /// The interaction model this terminal supports.
    fn capabilities(&self) -> Capabilities;

    /// Read one whole line (line-driven terminals).
    ///
    /// Returns `Ok(None)` at end of input so callers surface a typed "input
    /// closed" error instead of hanging.
    ///
    /// # Errors
    ///
    /// Returns an error when the underlying reader fails, or when the terminal
    /// is key-driven and does not support line reads.
    fn read_line(&mut self) -> AppResult<Option<String>>;

    /// Read one decoded keystroke (key-driven terminals).
    ///
    /// # Errors
    ///
    /// Returns an error when the underlying device fails, at end of input, or
    /// when the terminal is line-driven and does not support key reads.
    fn read_key(&mut self) -> AppResult<Key>;

    /// Write `text` verbatim (no trailing newline).
    ///
    /// # Errors
    ///
    /// Returns an error when the underlying writer fails.
    fn write(&mut self, text: &str) -> AppResult<()>;

    /// Write `text` followed by a newline (a carriage return + line feed in raw
    /// mode).
    ///
    /// # Errors
    ///
    /// Returns an error when the underlying writer fails.
    fn write_line(&mut self, text: &str) -> AppResult<()>;

    /// Flush any buffered output.
    ///
    /// # Errors
    ///
    /// Returns an error when the underlying writer fails.
    fn flush(&mut self) -> AppResult<()>;

    /// Move the cursor up `count` lines and clear from there down, so the next
    /// frame overwrites the previous one. A no-op for line-driven terminals.
    ///
    /// # Errors
    ///
    /// Returns an error when the underlying device fails.
    fn clear_last_lines(&mut self, count: u16) -> AppResult<()>;

    /// Enter interactive (raw) mode before a key-driven loop. A no-op for
    /// line-driven terminals.
    ///
    /// # Errors
    ///
    /// Returns an error when raw mode cannot be entered.
    fn begin_interactive(&mut self) -> AppResult<()>;

    /// Leave interactive (raw) mode after a key-driven loop. A no-op for
    /// line-driven terminals.
    ///
    /// # Errors
    ///
    /// Returns an error when raw mode cannot be restored.
    fn end_interactive(&mut self) -> AppResult<()>;
}

impl Terminal for Box<dyn Terminal> {
    fn capabilities(&self) -> Capabilities {
        (**self).capabilities()
    }

    fn read_line(&mut self) -> AppResult<Option<String>> {
        (**self).read_line()
    }

    fn read_key(&mut self) -> AppResult<Key> {
        (**self).read_key()
    }

    fn write(&mut self, text: &str) -> AppResult<()> {
        (**self).write(text)
    }

    fn write_line(&mut self, text: &str) -> AppResult<()> {
        (**self).write_line(text)
    }

    fn flush(&mut self) -> AppResult<()> {
        (**self).flush()
    }

    fn clear_last_lines(&mut self, count: u16) -> AppResult<()> {
        (**self).clear_last_lines(count)
    }

    fn begin_interactive(&mut self) -> AppResult<()> {
        (**self).begin_interactive()
    }

    fn end_interactive(&mut self) -> AppResult<()> {
        (**self).end_interactive()
    }
}
