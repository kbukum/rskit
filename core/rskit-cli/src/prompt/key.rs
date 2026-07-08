//! The keystroke vocabulary a key-driven [`Terminal`](crate::prompt::terminal::Terminal) yields.
//!
//! [`Key`] is rskit's own, minimal key abstraction so no third-party terminal
//! type (crossterm, termion, …) leaks into the public prompt surface. The rich
//! terminal maps platform events onto it; the scripted terminal feeds canned
//! sequences of it in tests.

/// A single decoded keystroke from an interactive terminal.
///
/// Only the keys the prompt widgets act on are modelled; anything else decodes
/// to [`Key::Unknown`] so callers can ignore it without a catch-all on a
/// platform enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Key {
    /// Confirm the current answer (Enter / Return).
    Enter,
    /// Space bar — toggles the focused item in multi-select, or inserts a
    /// literal space in text entry.
    Space,
    /// Delete the character before the cursor (Backspace).
    Backspace,
    /// Abandon the prompt (Escape).
    Escape,
    /// Advance focus (Tab).
    Tab,
    /// Move focus up / to the previous item.
    Up,
    /// Move focus down / to the next item.
    Down,
    /// Move the text cursor left.
    Left,
    /// Move the text cursor right.
    Right,
    /// Jump to the first item / start of line.
    Home,
    /// Jump to the last item / end of line.
    End,
    /// A printable character was typed.
    Char(char),
    /// A cancellation request (Ctrl+C / Ctrl+D).
    Interrupt,
    /// A key with no prompt-relevant meaning.
    Unknown,
}

impl Key {
    /// Whether this key represents a cancellation request.
    #[must_use]
    pub const fn is_interrupt(self) -> bool {
        matches!(self, Self::Interrupt)
    }
}

#[cfg(test)]
mod tests {
    use super::Key;

    #[test]
    fn only_interrupt_reports_as_interrupt() {
        assert!(Key::Interrupt.is_interrupt());
        for key in [
            Key::Enter,
            Key::Escape,
            Key::Char('a'),
            Key::Space,
            Key::Unknown,
        ] {
            assert!(!key.is_interrupt());
        }
    }
}
