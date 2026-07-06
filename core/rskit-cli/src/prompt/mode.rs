//! How a [`Prompter`](crate::prompt::Prompter) sources answers.

use std::io::{self, IsTerminal};

/// How the prompter sources answers, resolved once up front.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PromptMode {
    /// stdin is a terminal: render prompts and read typed answers.
    Interactive,
    /// stdin is not a terminal (CI, piped): never block; resolve to defaults.
    NonInteractive,
}

impl PromptMode {
    /// Resolve the mode from the current process stdin's TTY status.
    #[must_use]
    pub fn from_stdin() -> Self {
        Self::from_terminal(io::stdin().is_terminal())
    }

    /// Resolve the mode from an already-known terminal status (env-free, testable).
    #[must_use]
    pub const fn from_terminal(is_terminal: bool) -> Self {
        if is_terminal {
            Self::Interactive
        } else {
            Self::NonInteractive
        }
    }

    /// Whether this mode reads live answers.
    #[must_use]
    pub const fn is_interactive(self) -> bool {
        matches!(self, Self::Interactive)
    }
}

#[cfg(test)]
mod tests {
    use super::PromptMode;

    #[test]
    fn mode_resolves_from_terminal_status() {
        assert_eq!(PromptMode::from_terminal(true), PromptMode::Interactive);
        assert_eq!(PromptMode::from_terminal(false), PromptMode::NonInteractive);
        assert!(PromptMode::Interactive.is_interactive());
        assert!(!PromptMode::NonInteractive.is_interactive());
    }
}
