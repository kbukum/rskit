//! How a [`Prompter`](crate::prompt::Prompter) sources answers.

use std::io::{self, IsTerminal};

/// How the prompter sources answers, resolved once up front.
///
/// The decision models *interactive stdio*: prompts are read from stdin but
/// rendered to stderr, so a session is only interactive when **both** streams
/// are terminals. If stderr is redirected (`cmd 2>log`) the user would never
/// see the question, so the mode falls back to [`NonInteractive`] even when
/// stdin is a TTY.
///
/// [`NonInteractive`]: PromptMode::NonInteractive
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PromptMode {
    /// Interactive stdio (both stdin and stderr are terminals): render prompts
    /// and read live, typed answers.
    Interactive,
    /// Non-interactive stdio (CI, piped, or a redirected prompt sink): never
    /// block; resolve each question to its declared default.
    NonInteractive,
}

impl PromptMode {
    /// Resolve the mode from the current process's stdin and stderr TTY status.
    #[must_use]
    pub fn from_env() -> Self {
        Self::from_stdio(io::stdin().is_terminal(), io::stderr().is_terminal())
    }

    /// Resolve the mode from already-known stream TTY statuses (env-free, testable).
    ///
    /// Interactive only when both the input (stdin) and the prompt sink (stderr)
    /// are terminals, so a redirected sink never leaves the user blocked behind
    /// an invisible prompt.
    #[must_use]
    pub const fn from_stdio(stdin_is_tty: bool, stderr_is_tty: bool) -> Self {
        if stdin_is_tty && stderr_is_tty {
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
    fn interactive_requires_both_stdin_and_stderr_terminals() {
        assert_eq!(PromptMode::from_stdio(true, true), PromptMode::Interactive);
        assert_eq!(
            PromptMode::from_stdio(true, false),
            PromptMode::NonInteractive
        );
        assert_eq!(
            PromptMode::from_stdio(false, true),
            PromptMode::NonInteractive
        );
        assert_eq!(
            PromptMode::from_stdio(false, false),
            PromptMode::NonInteractive
        );
        assert!(PromptMode::Interactive.is_interactive());
        assert!(!PromptMode::NonInteractive.is_interactive());
    }

    #[test]
    fn from_env_resolves_against_the_process_stdio() {
        // Under a test harness stdin/stderr are not TTYs, so the mode resolves
        // to NonInteractive; the assertion also proves `from_env` is callable.
        assert_eq!(PromptMode::from_env(), PromptMode::NonInteractive);
    }
}
