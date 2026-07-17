use rskit_errors::AppResult;

use super::Capabilities;
use crate::prompt::key::Key;

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
