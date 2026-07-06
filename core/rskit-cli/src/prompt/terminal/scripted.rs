//! A deterministic in-memory [`Terminal`] for tests and examples.
//!
//! `ScriptedTerminal` is a first-class injectable double, not a test-only escape
//! hatch: it plays a canned sequence of [`Key`]s or lines and records everything
//! written, so both the key-driven and line-driven prompt paths can be exercised
//! without a real terminal. Choose the interaction model with
//! [`ScriptedTerminal::key_driven`] or [`ScriptedTerminal::line_driven`], queue
//! input with the `with_*` builders, then read back rendered output via
//! [`ScriptedTerminal::output`].

use std::collections::VecDeque;

use rskit_errors::{AppError, AppResult};

use super::{Capabilities, Terminal};
use crate::prompt::key::Key;

/// One scripted input event: a whole line or a single key.
#[derive(Debug, Clone)]
enum Input {
    Line(String),
    Key(Key),
}

/// A scripted, in-memory terminal that replays canned input and captures output.
#[derive(Debug, Default)]
pub struct ScriptedTerminal {
    capabilities: Capabilities,
    inputs: VecDeque<Input>,
    output: String,
    raw: bool,
}

impl ScriptedTerminal {
    /// A key-driven scripted terminal (advertises key input, like a raw TTY).
    #[must_use]
    pub fn key_driven() -> Self {
        Self {
            capabilities: Capabilities::key_driven(),
            ..Self::default()
        }
    }

    /// A line-driven scripted terminal (advertises line input, like cooked stdio).
    #[must_use]
    pub fn line_driven() -> Self {
        Self {
            capabilities: Capabilities::line_driven(),
            ..Self::default()
        }
    }

    /// Queue one input line (line-driven scripts).
    #[must_use]
    pub fn with_line(mut self, line: impl Into<String>) -> Self {
        self.inputs.push_back(Input::Line(line.into()));
        self
    }

    /// Queue several input lines in order.
    #[must_use]
    pub fn with_lines<I, S>(mut self, lines: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for line in lines {
            self.inputs.push_back(Input::Line(line.into()));
        }
        self
    }

    /// Queue one keystroke (key-driven scripts).
    #[must_use]
    pub fn with_key(mut self, key: Key) -> Self {
        self.inputs.push_back(Input::Key(key));
        self
    }

    /// Queue several keystrokes in order.
    #[must_use]
    pub fn with_keys<I>(mut self, keys: I) -> Self
    where
        I: IntoIterator<Item = Key>,
    {
        for key in keys {
            self.inputs.push_back(Input::Key(key));
        }
        self
    }

    /// Everything written to the terminal so far, for assertions.
    #[must_use]
    pub fn output(&self) -> &str {
        &self.output
    }

    /// Whether the terminal is currently in interactive (raw) mode.
    #[must_use]
    pub const fn is_interactive(&self) -> bool {
        self.raw
    }
}

impl Terminal for ScriptedTerminal {
    fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    fn read_line(&mut self) -> AppResult<Option<String>> {
        match self.inputs.pop_front() {
            Some(Input::Line(line)) => Ok(Some(line)),
            Some(Input::Key(_)) => Err(AppError::invalid_input(
                "prompt",
                "scripted terminal expected a line but the next input is a key",
            )),
            None => Ok(None),
        }
    }

    fn read_key(&mut self) -> AppResult<Key> {
        match self.inputs.pop_front() {
            Some(Input::Key(key)) => Ok(key),
            Some(Input::Line(_)) => Err(AppError::invalid_input(
                "prompt",
                "scripted terminal expected a key but the next input is a line",
            )),
            None => Err(AppError::invalid_input(
                "prompt",
                "scripted terminal ran out of keys",
            )),
        }
    }

    fn write(&mut self, text: &str) -> AppResult<()> {
        self.output.push_str(text);
        Ok(())
    }

    fn write_line(&mut self, text: &str) -> AppResult<()> {
        self.output.push_str(text);
        self.output.push('\n');
        Ok(())
    }

    fn flush(&mut self) -> AppResult<()> {
        Ok(())
    }

    fn clear_last_lines(&mut self, _count: u16) -> AppResult<()> {
        Ok(())
    }

    fn begin_interactive(&mut self) -> AppResult<()> {
        self.raw = true;
        Ok(())
    }

    fn end_interactive(&mut self) -> AppResult<()> {
        self.raw = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Key, ScriptedTerminal, Terminal};

    #[test]
    fn replays_lines_then_eof() {
        let mut term = ScriptedTerminal::line_driven().with_lines(["one", "two"]);
        assert_eq!(term.read_line().expect("line"), Some("one".to_string()));
        assert_eq!(term.read_line().expect("line"), Some("two".to_string()));
        assert_eq!(term.read_line().expect("eof"), None);
    }

    #[test]
    fn replays_keys_and_captures_output() {
        let mut term = ScriptedTerminal::key_driven().with_keys([Key::Down, Key::Enter]);
        term.write_line("frame").expect("write");
        assert_eq!(term.read_key().expect("key"), Key::Down);
        assert_eq!(term.read_key().expect("key"), Key::Enter);
        assert!(term.read_key().is_err());
        assert_eq!(term.output(), "frame\n");
    }

    #[test]
    fn interactive_lifecycle_toggles() {
        let mut term = ScriptedTerminal::key_driven();
        assert!(!term.is_interactive());
        term.begin_interactive().expect("begin");
        assert!(term.is_interactive());
        term.end_interactive().expect("end");
        assert!(!term.is_interactive());
    }
}
