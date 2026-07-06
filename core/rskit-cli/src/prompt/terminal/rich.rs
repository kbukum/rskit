//! A raw-mode terminal over [crossterm], enabling live arrow-key navigation.
//!
//! `RichTerminal` is compiled only when the `interactive` feature is enabled. It
//! puts the controlling terminal into raw mode for the duration of a key-driven
//! prompt, decodes platform key events into rskit's own [`Key`] vocabulary, and
//! redraws frames in place via cursor movement. Raw mode is restored on
//! [`Terminal::end_interactive`] and, as a panic-safety net, on drop.
//!
//! It is not unit-tested (it needs a real TTY); the shared prompt-kind logic is
//! covered through [`ScriptedTerminal`](super::ScriptedTerminal), and a PTY smoke
//! test exercises the raw-mode path end to end.

use std::io::{self, IsTerminal, Write};

use crossterm::cursor::MoveToPreviousLine;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, read};
use crossterm::terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode};
use crossterm::{execute, queue};
use rskit_errors::{AppError, AppResult};

use super::{Capabilities, Terminal};
use crate::prompt::key::Key;

/// A raw-mode [`Terminal`] that reads individual keys and redraws frames.
///
/// Render output is written to stderr, matching the stream prompts style against.
pub struct RichTerminal {
    writer: io::Stderr,
    raw: bool,
}

impl RichTerminal {
    /// Build a rich terminal rendering to stderr.
    ///
    /// # Errors
    ///
    /// Returns an error unless both stderr and stdin are terminals: frames are
    /// drawn to stderr, while raw-mode key input is read from stdin, so both
    /// streams must be a real TTY.
    pub fn stderr() -> AppResult<Self> {
        let writer = io::stderr();
        if !writer.is_terminal() {
            return Err(AppError::invalid_input(
                "prompt",
                "rich terminal requires an interactive stderr",
            ));
        }
        if !io::stdin().is_terminal() {
            return Err(AppError::invalid_input(
                "prompt",
                "rich terminal requires an interactive stdin for key input",
            ));
        }
        Ok(Self { writer, raw: false })
    }
}

impl Drop for RichTerminal {
    fn drop(&mut self) {
        if self.raw {
            let _ = disable_raw_mode();
        }
    }
}

const fn map_key(event: KeyEvent) -> Key {
    let ctrl = event.modifiers.contains(KeyModifiers::CONTROL);
    match event.code {
        KeyCode::Char('c' | 'd') if ctrl => Key::Interrupt,
        KeyCode::Char(' ') => Key::Space,
        KeyCode::Char(c) => Key::Char(c),
        KeyCode::Enter => Key::Enter,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Esc => Key::Escape,
        KeyCode::Tab => Key::Tab,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        _ => Key::Unknown,
    }
}

impl Terminal for RichTerminal {
    fn capabilities(&self) -> Capabilities {
        Capabilities::key_driven()
    }

    fn read_line(&mut self) -> AppResult<Option<String>> {
        Err(AppError::invalid_input(
            "prompt",
            "rich terminal reads keys, not lines",
        ))
    }

    fn read_key(&mut self) -> AppResult<Key> {
        loop {
            match read().map_err(AppError::internal)? {
                Event::Key(event) if event.kind == KeyEventKind::Press => {
                    return Ok(map_key(event));
                }
                _ => {}
            }
        }
    }

    fn write(&mut self, text: &str) -> AppResult<()> {
        write!(self.writer, "{text}").map_err(AppError::internal)
    }

    fn write_line(&mut self, text: &str) -> AppResult<()> {
        write!(self.writer, "{text}\r\n").map_err(AppError::internal)
    }

    fn flush(&mut self) -> AppResult<()> {
        self.writer.flush().map_err(AppError::internal)
    }

    fn clear_last_lines(&mut self, count: u16) -> AppResult<()> {
        if count == 0 {
            return Ok(());
        }
        queue!(
            self.writer,
            MoveToPreviousLine(count),
            Clear(ClearType::FromCursorDown)
        )
        .map_err(AppError::internal)?;
        self.flush()
    }

    fn begin_interactive(&mut self) -> AppResult<()> {
        if !self.raw {
            enable_raw_mode().map_err(AppError::internal)?;
            self.raw = true;
        }
        Ok(())
    }

    fn end_interactive(&mut self) -> AppResult<()> {
        if self.raw {
            disable_raw_mode().map_err(AppError::internal)?;
            self.raw = false;
        }
        execute!(self.writer, Clear(ClearType::UntilNewLine)).map_err(AppError::internal)
    }
}

#[cfg(test)]
mod tests {
    use super::{Key, KeyCode, KeyEvent, KeyModifiers, RichTerminal, map_key};

    #[test]
    fn requires_a_terminal_stderr() {
        // Under the test harness stderr is not a TTY, so construction is refused
        // rather than putting a non-terminal into raw mode.
        assert!(RichTerminal::stderr().is_err());
    }

    #[test]
    fn maps_control_keys_to_interrupt() {
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(map_key(ctrl_c), Key::Interrupt);
        let plain_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE);
        assert_eq!(map_key(plain_c), Key::Char('c'));
    }

    #[test]
    fn maps_space_bar_to_space_not_char() {
        // Regression: the space bar arrives as KeyCode::Char(' '); it must decode
        // to Key::Space so multi-select's toggle arm is reachable on a real TTY.
        // Feeding Key::Space from a scripted double is not enough — the decoder
        // itself must emit it.
        let space = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE);
        assert_eq!(map_key(space), Key::Space);
    }

    #[test]
    fn maps_navigation_keys() {
        assert_eq!(map_key(KeyEvent::from(KeyCode::Up)), Key::Up);
        assert_eq!(map_key(KeyEvent::from(KeyCode::Enter)), Key::Enter);
        assert_eq!(map_key(KeyEvent::from(KeyCode::Backspace)), Key::Backspace);
        assert_eq!(map_key(KeyEvent::from(KeyCode::Esc)), Key::Escape);
        assert_eq!(map_key(KeyEvent::from(KeyCode::F(5))), Key::Unknown);
    }
}
