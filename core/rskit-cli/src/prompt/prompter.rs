//! The [`Prompter`]: a terminal-agnostic driver for every prompt kind.
//!
//! A `Prompter` binds three things — a [`Terminal`], a resolved [`PromptMode`],
//! and a rendering [`Style`] — and exposes one method per question type.
//! The prompt-kind logic lives in [`super::kinds`];
//! the prompter only wires the shared state through to it, so the same call works over cooked stdio,
//! a raw-mode TTY, or a scripted test double.
//!
//! Build one from the environment with [`Prompter::from_env`] —
//! which auto-selects a rich raw-mode terminal when one is available
//! and the `interactive` feature is compiled, else a line terminal —
//! or from explicit parts with [`Prompter::new`] for deterministic tests.

use std::io::IsTerminal;

use rskit_errors::AppResult;

use super::choice::{Choice, ChoiceId};
use super::kinds;
use super::mode::PromptMode;
use super::render::Style;
use super::terminal::{LineTerminal, Terminal};
use super::validate::Validator;
use crate::theme::{ColorChoice, Glyphs, Palette};

/// A terminal-agnostic prompt driver.
///
/// Generic over its [`Terminal`]
/// so tests can bind a [`ScriptedTerminal`](super::terminal::ScriptedTerminal) directly while [`Prompter::from_env`] erases the concrete terminal behind a `Box<dyn Terminal>`.
pub struct Prompter<T> {
    terminal: T,
    mode: PromptMode,
    style: Style,
}

impl Prompter<Box<dyn Terminal>> {
    /// Build a prompter bound to the process environment.
    ///
    /// The [`PromptMode`] follows whether both stdin and stderr are terminals,
    /// and the [`Palette`] follows `color` against stderr, so interactivity
    /// and styling both honour redirection and `NO_COLOR`. Prompts render to stderr,
    /// so a redirected stderr (e.g. `cmd 2>log`) forces [`PromptMode::NonInteractive`] rather than blocking on input behind an invisible prompt.
    /// When the `interactive` feature is compiled and both streams are terminals,
    /// a rich raw-mode terminal is selected for arrow-key navigation; otherwise a line terminal is used.
    #[must_use]
    pub fn from_env(color: ColorChoice) -> Self {
        let stderr = std::io::stderr();
        let mode = PromptMode::from_stdio(std::io::stdin().is_terminal(), stderr.is_terminal());
        let palette = Palette::for_stream(color, &stderr);
        let glyphs = Glyphs::from_env();
        let terminal = resolve_terminal(mode.is_interactive());
        Self {
            terminal,
            mode,
            style: Style::new(palette, glyphs),
        }
    }
}

impl<T: Terminal> Prompter<T> {
    /// Build a prompter from an explicit terminal, mode, and palette.
    ///
    /// Glyphs default to the ASCII fallback for byte-clean, deterministic tests;
    /// override with [`Prompter::with_glyphs`].
    #[must_use]
    pub const fn new(terminal: T, mode: PromptMode, palette: Palette) -> Self {
        Self {
            terminal,
            mode,
            style: Style::new(palette, Glyphs::new(false)),
        }
    }

    /// Override the glyph set (Unicode symbols vs ASCII fallback).
    #[must_use]
    pub const fn with_glyphs(mut self, glyphs: Glyphs) -> Self {
        self.style = Style::new(self.style.palette(), glyphs);
        self
    }

    /// The resolved interaction mode.
    #[must_use]
    pub const fn mode(&self) -> PromptMode {
        self.mode
    }

    /// The bound terminal, for inspecting captured output in tests.
    #[must_use]
    pub const fn terminal(&self) -> &T {
        &self.terminal
    }

    /// Build the invariant presentation context for a prompt from the bound
    /// style and mode.
    const fn ask<'a>(&self, prompt: &'a str) -> kinds::Ask<'a> {
        kinds::Ask {
            style: self.style,
            mode: self.mode,
            prompt,
        }
    }

    /// Ask for exactly one choice.
    ///
    /// In [`PromptMode::NonInteractive`] this resolves to the recommended choice;
    /// with none it is a typed error. A key-driven terminal shows an arrow-key radio list;
    /// a line-driven terminal shows a numbered list.
    ///
    /// # Errors
    ///
    /// Returns an error when `choices` is empty, when a non-interactive prompt has no recommended default,
    /// when the user cancels, or when input closes early.
    pub fn select(&mut self, prompt: &str, choices: &[Choice]) -> AppResult<ChoiceId> {
        let ask = self.ask(prompt);
        kinds::select::run(&mut self.terminal, ask, choices)
    }

    /// Ask for zero or more choices.
    ///
    /// The default answer is the set of recommended choices, which may be empty.
    /// A key-driven terminal shows an arrow-key checkbox list;
    /// a line-driven terminal accepts a comma-separated list of numbers.
    ///
    /// # Errors
    ///
    /// Returns an error when `choices` is empty, when the user cancels, or when input closes early.
    pub fn multi_select(&mut self, prompt: &str, choices: &[Choice]) -> AppResult<Vec<ChoiceId>> {
        let ask = self.ask(prompt);
        kinds::multi_select::run(&mut self.terminal, ask, choices)
    }

    /// Ask a yes/no question with an explicit default.
    ///
    /// # Errors
    ///
    /// Returns an error when the user cancels or when input closes early.
    pub fn confirm(&mut self, prompt: &str, default: bool) -> AppResult<bool> {
        let ask = self.ask(prompt);
        kinds::confirm::run(&mut self.terminal, ask, default)
    }

    /// Ask for freeform text with an optional default.
    ///
    /// In [`PromptMode::NonInteractive`] this resolves to `default`; with none it is a typed error.
    ///
    /// # Errors
    ///
    /// Returns an error when a non-interactive prompt has no default, when the user cancels,
    /// or when input closes early.
    pub fn text(&mut self, prompt: &str, default: Option<&str>) -> AppResult<String> {
        let ask = self.ask(prompt);
        kinds::text::run(&mut self.terminal, ask, default, None)
    }

    /// Ask for freeform text validated by `validator`, re-asking on rejection.
    ///
    /// In [`PromptMode::NonInteractive`] a rejected default is a typed error rather than a silent bad value.
    ///
    /// # Errors
    ///
    /// Returns an error when a non-interactive prompt has no default or a rejected default,
    /// when the user cancels, or when input closes early.
    pub fn text_with(
        &mut self,
        prompt: &str,
        default: Option<&str>,
        validator: &dyn Validator,
    ) -> AppResult<String> {
        let ask = self.ask(prompt);
        kinds::text::run(&mut self.terminal, ask, default, Some(validator))
    }
}

/// Select the concrete terminal for [`Prompter::from_env`]: rich when a TTY is present
/// and the `interactive` feature is compiled, else line.
fn resolve_terminal(is_tty: bool) -> Box<dyn Terminal> {
    #[cfg(feature = "interactive")]
    {
        if is_tty && let Ok(terminal) = super::terminal::RichTerminal::stderr() {
            return Box::new(terminal);
        }
    }
    #[cfg(not(feature = "interactive"))]
    let _ = is_tty;
    Box::new(LineTerminal::stdio())
}

#[cfg(test)]
mod tests {
    use super::{Choice, ChoiceId, Palette, PromptMode, Prompter};
    use crate::prompt::key::Key;
    use crate::prompt::terminal::ScriptedTerminal;
    use crate::prompt::validate::non_empty;
    use crate::theme::{ColorChoice, Glyphs};

    fn plain_choices() -> Vec<Choice> {
        vec![
            Choice::new("go", "Go"),
            Choice::new("rust", "Rust").recommended(),
            Choice::new("node", "Node.js"),
        ]
    }

    fn no_default_choices() -> Vec<Choice> {
        vec![Choice::new("go", "Go"), Choice::new("rust", "Rust")]
    }

    fn line_prompter(
        terminal: ScriptedTerminal,
        mode: PromptMode,
        color: bool,
    ) -> Prompter<ScriptedTerminal> {
        Prompter::new(terminal, mode, Palette::new(color))
    }

    // ── Non-interactive resolution ──────────────────────────────────────

    #[test]
    fn non_interactive_select_resolves_to_recommended() {
        let choice = line_prompter(
            ScriptedTerminal::line_driven(),
            PromptMode::NonInteractive,
            false,
        )
        .select("Ecosystem?", &plain_choices())
        .expect("recommended default resolves");
        assert_eq!(choice, ChoiceId::new("rust"));
    }

    #[test]
    fn non_interactive_select_without_default_errors() {
        let choices = vec![Choice::new("go", "Go"), Choice::new("rust", "Rust")];
        let err = line_prompter(
            ScriptedTerminal::line_driven(),
            PromptMode::NonInteractive,
            false,
        )
        .select("Ecosystem?", &choices)
        .expect_err("no recommended default must error");
        assert!(err.message().contains("non-interactive"));
    }

    #[test]
    fn non_interactive_multi_select_returns_recommended_set() {
        let selected = line_prompter(
            ScriptedTerminal::line_driven(),
            PromptMode::NonInteractive,
            false,
        )
        .multi_select("Tasks?", &plain_choices())
        .expect("recommended set resolves");
        assert_eq!(selected, vec![ChoiceId::new("rust")]);
    }

    #[test]
    fn non_interactive_multi_select_allows_empty_default() {
        let choices = vec![Choice::new("a", "A"), Choice::new("b", "B")];
        let selected = line_prompter(
            ScriptedTerminal::line_driven(),
            PromptMode::NonInteractive,
            false,
        )
        .multi_select("Tasks?", &choices)
        .expect("empty selection is valid");
        assert!(selected.is_empty());
    }

    #[test]
    fn non_interactive_confirm_returns_default() {
        let value = line_prompter(
            ScriptedTerminal::line_driven(),
            PromptMode::NonInteractive,
            false,
        )
        .confirm("Proceed?", true)
        .expect("confirm resolves to default");
        assert!(value);
    }

    #[test]
    fn non_interactive_text_uses_default_and_errors_without_one() {
        let value = line_prompter(
            ScriptedTerminal::line_driven(),
            PromptMode::NonInteractive,
            false,
        )
        .text("Name?", Some("toven"))
        .expect("text resolves to default");
        assert_eq!(value, "toven");

        let err = line_prompter(
            ScriptedTerminal::line_driven(),
            PromptMode::NonInteractive,
            false,
        )
        .text("Name?", None)
        .expect_err("no default must error");
        assert!(err.message().contains("non-interactive"));
    }

    #[test]
    fn non_interactive_text_with_rejects_invalid_default() {
        let err = line_prompter(
            ScriptedTerminal::line_driven(),
            PromptMode::NonInteractive,
            false,
        )
        .text_with("Name?", Some("  "), &non_empty("required"))
        .expect_err("invalid default must error");
        assert!(err.message().contains("invalid"));
    }

    // ── Line-driven interaction ─────────────────────────────────────────

    #[test]
    fn line_select_reads_a_numbered_answer() {
        let choice = line_prompter(
            ScriptedTerminal::line_driven().with_line("1"),
            PromptMode::Interactive,
            false,
        )
        .select("Ecosystem?", &plain_choices())
        .expect("first choice selected");
        assert_eq!(choice, ChoiceId::new("go"));
    }

    #[test]
    fn line_select_empty_line_accepts_recommended() {
        let choice = line_prompter(
            ScriptedTerminal::line_driven().with_line(""),
            PromptMode::Interactive,
            false,
        )
        .select("Ecosystem?", &plain_choices())
        .expect("blank accepts recommended");
        assert_eq!(choice, ChoiceId::new("rust"));
    }

    #[test]
    fn line_select_reprompts_on_invalid_then_succeeds() {
        let mut prompter = line_prompter(
            ScriptedTerminal::line_driven().with_lines(["9", "x", "3"]),
            PromptMode::Interactive,
            false,
        );
        let choice = prompter
            .select("Ecosystem?", &plain_choices())
            .expect("valid choice after retries");
        assert_eq!(choice, ChoiceId::new("node"));
        assert!(prompter.terminal().output().contains("between 1 and 3"));
    }

    #[test]
    fn line_select_errors_when_input_closes() {
        let choices = vec![Choice::new("go", "Go"), Choice::new("rust", "Rust")];
        let err = line_prompter(
            ScriptedTerminal::line_driven(),
            PromptMode::Interactive,
            false,
        )
        .select("Ecosystem?", &choices)
        .expect_err("closed input must error, not hang");
        assert!(err.message().contains("input closed"));
    }

    #[test]
    fn line_multi_select_parses_and_dedupes() {
        let selected = line_prompter(
            ScriptedTerminal::line_driven().with_line("3, 1, 1"),
            PromptMode::Interactive,
            false,
        )
        .multi_select("Tasks?", &plain_choices())
        .expect("comma list parses");
        assert_eq!(selected, vec![ChoiceId::new("node"), ChoiceId::new("go")]);
    }

    #[test]
    fn line_multi_select_empty_line_uses_defaults() {
        let selected = line_prompter(
            ScriptedTerminal::line_driven().with_line(""),
            PromptMode::Interactive,
            false,
        )
        .multi_select("Tasks?", &plain_choices())
        .expect("blank uses recommended defaults");
        assert_eq!(selected, vec![ChoiceId::new("rust")]);
    }

    #[test]
    fn line_confirm_parses_yes_no_and_default() {
        assert!(
            line_prompter(
                ScriptedTerminal::line_driven().with_line("y"),
                PromptMode::Interactive,
                false
            )
            .confirm("Proceed?", false)
            .expect("yes")
        );
        assert!(
            !line_prompter(
                ScriptedTerminal::line_driven().with_line("no"),
                PromptMode::Interactive,
                false
            )
            .confirm("Proceed?", true)
            .expect("no")
        );
        assert!(
            line_prompter(
                ScriptedTerminal::line_driven().with_line(""),
                PromptMode::Interactive,
                false
            )
            .confirm("Proceed?", true)
            .expect("blank uses default")
        );
    }

    #[test]
    fn line_text_reads_value_and_falls_back_to_default() {
        let value = line_prompter(
            ScriptedTerminal::line_driven().with_line("custom"),
            PromptMode::Interactive,
            false,
        )
        .text("Name?", Some("toven"))
        .expect("typed value");
        assert_eq!(value, "custom");

        let value = line_prompter(
            ScriptedTerminal::line_driven().with_line(""),
            PromptMode::Interactive,
            false,
        )
        .text("Name?", Some("toven"))
        .expect("blank uses default");
        assert_eq!(value, "toven");
    }

    #[test]
    fn line_text_with_reasks_until_valid() {
        let mut prompter = line_prompter(
            ScriptedTerminal::line_driven().with_lines(["  ", "ok"]),
            PromptMode::Interactive,
            false,
        );
        let value = prompter
            .text_with("Name?", None, &non_empty("required"))
            .expect("valid after retry");
        assert_eq!(value, "ok");
        assert!(prompter.terminal().output().contains("required"));
    }

    // ── Key-driven interaction ──────────────────────────────────────────

    #[test]
    fn key_select_navigates_and_confirms() {
        let choice = line_prompter(
            ScriptedTerminal::key_driven().with_keys([Key::Down, Key::Enter]),
            PromptMode::Interactive,
            false,
        )
        .select("Ecosystem?", &plain_choices())
        .expect("arrow navigation selects");
        assert_eq!(choice, ChoiceId::new("node"));
    }

    #[test]
    fn key_select_starts_on_recommended_default() {
        let choice = line_prompter(
            ScriptedTerminal::key_driven().with_key(Key::Enter),
            PromptMode::Interactive,
            false,
        )
        .select("Ecosystem?", &plain_choices())
        .expect("enter accepts default");
        assert_eq!(choice, ChoiceId::new("rust"));
    }

    #[test]
    fn key_select_escape_cancels() {
        let err = line_prompter(
            ScriptedTerminal::key_driven().with_key(Key::Escape),
            PromptMode::Interactive,
            false,
        )
        .select("Ecosystem?", &plain_choices())
        .expect_err("escape cancels");
        assert!(err.message().contains("cancelled"));
    }

    #[test]
    fn key_multi_select_toggles_selection() {
        // Recommended (Rust, index 1) starts selected; toggle Go on, Rust off, Node on,
        // to exercise toggling in both directions.
        let selected = line_prompter(
            ScriptedTerminal::key_driven().with_keys([
                Key::Space, // Go on
                Key::Down,
                Key::Space, // Rust off
                Key::Down,
                Key::Space, // Node on
                Key::Enter,
            ]),
            PromptMode::Interactive,
            false,
        )
        .multi_select("Tasks?", &plain_choices())
        .expect("space toggles");
        assert_eq!(selected, vec![ChoiceId::new("go"), ChoiceId::new("node")]);
    }

    #[test]
    fn key_confirm_reads_letter() {
        assert!(
            line_prompter(
                ScriptedTerminal::key_driven().with_key(Key::Char('y')),
                PromptMode::Interactive,
                false
            )
            .confirm("Proceed?", false)
            .expect("y confirms")
        );
    }

    #[test]
    fn key_text_edits_with_backspace() {
        let value = line_prompter(
            ScriptedTerminal::key_driven().with_keys([
                Key::Char('h'),
                Key::Char('i'),
                Key::Char('x'),
                Key::Backspace,
                Key::Enter,
            ]),
            PromptMode::Interactive,
            false,
        )
        .text("Name?", None)
        .expect("typed value");
        assert_eq!(value, "hi");
    }

    #[test]
    fn key_text_inserts_literal_space() {
        // The space bar decodes to Key::Space (not Key::Char(' ')),
        // so text entry must treat it as a literal space —
        // otherwise multi-word answers are impossible on a real rich terminal.
        let value = line_prompter(
            ScriptedTerminal::key_driven().with_keys([
                Key::Char('h'),
                Key::Char('i'),
                Key::Space,
                Key::Char('t'),
                Key::Char('h'),
                Key::Char('e'),
                Key::Char('r'),
                Key::Char('e'),
                Key::Enter,
            ]),
            PromptMode::Interactive,
            false,
        )
        .text("Name?", None)
        .expect("typed value");
        assert_eq!(value, "hi there");
    }

    #[test]
    fn key_select_runs_in_raw_mode_and_restores() {
        let mut prompter = line_prompter(
            ScriptedTerminal::key_driven().with_key(Key::Enter),
            PromptMode::Interactive,
            false,
        );
        let _ = prompter
            .select("Ecosystem?", &plain_choices())
            .expect("select");
        assert!(!prompter.terminal().is_interactive(), "raw mode restored");
    }

    // ── Rendering & metadata ────────────────────────────────────────────

    #[test]
    fn choice_metadata_round_trips_into_rendered_output() {
        let choices = vec![
            Choice::new("rust", "Rust")
                .with_annotation("detected in dev-deps")
                .recommended(),
            Choice::new("go", "Go"),
        ];
        let mut prompter = line_prompter(
            ScriptedTerminal::line_driven().with_line("1"),
            PromptMode::Interactive,
            false,
        );
        let _ = prompter.select("Ecosystem?", &choices).expect("select");
        let rendered = prompter.terminal().output();
        assert!(rendered.contains("Rust"));
        assert!(rendered.contains("detected in dev-deps"));
        assert!(rendered.contains("(recommended)"));
    }

    #[test]
    fn disabled_palette_renders_without_ansi_escapes() {
        let mut prompter = line_prompter(
            ScriptedTerminal::line_driven().with_line("1"),
            PromptMode::Interactive,
            false,
        );
        let _ = prompter
            .select("Ecosystem?", &plain_choices())
            .expect("select");
        assert!(
            !prompter.terminal().output().contains('\u{1b}'),
            "no color must be byte-clean"
        );
    }

    #[test]
    fn enabled_palette_emits_ansi_escapes() {
        let mut prompter = line_prompter(
            ScriptedTerminal::line_driven().with_line("1"),
            PromptMode::Interactive,
            true,
        );
        let _ = prompter
            .select("Ecosystem?", &plain_choices())
            .expect("select");
        assert!(
            prompter.terminal().output().contains('\u{1b}'),
            "color must emit SGR escapes"
        );
    }

    #[test]
    fn empty_choice_set_is_rejected() {
        let err = line_prompter(
            ScriptedTerminal::line_driven(),
            PromptMode::Interactive,
            false,
        )
        .select("Ecosystem?", &[])
        .expect_err("empty choices rejected");
        assert!(err.message().contains("at least one choice"));
    }

    #[test]
    fn multi_select_empty_choice_set_is_rejected() {
        let err = line_prompter(
            ScriptedTerminal::line_driven(),
            PromptMode::Interactive,
            false,
        )
        .multi_select("Tasks?", &[])
        .expect_err("empty choices rejected");
        assert!(err.message().contains("at least one choice"));
    }

    // ── Confirm: key- and line-driven branches ──────────────────────────

    #[test]
    fn key_confirm_enter_default_letter_no_and_escape() {
        // Enter accepts the default.
        assert!(
            line_prompter(
                ScriptedTerminal::key_driven().with_key(Key::Enter),
                PromptMode::Interactive,
                false
            )
            .confirm("Proceed?", true)
            .expect("enter accepts default")
        );
        // 'n' declines.
        assert!(
            !line_prompter(
                ScriptedTerminal::key_driven().with_key(Key::Char('n')),
                PromptMode::Interactive,
                false
            )
            .confirm("Proceed?", true)
            .expect("n declines")
        );
        // An unrelated key is ignored before 'Y' confirms.
        assert!(
            line_prompter(
                ScriptedTerminal::key_driven().with_keys([Key::Left, Key::Char('Y')]),
                PromptMode::Interactive,
                false
            )
            .confirm("Proceed?", false)
            .expect("Y confirms after an ignored key")
        );
        // Escape cancels.
        let err = line_prompter(
            ScriptedTerminal::key_driven().with_key(Key::Escape),
            PromptMode::Interactive,
            false,
        )
        .confirm("Proceed?", true)
        .expect_err("escape cancels");
        assert!(err.message().contains("cancelled"));
    }

    #[test]
    fn line_confirm_reprompts_on_invalid_and_errors_on_close() {
        let mut prompter = line_prompter(
            ScriptedTerminal::line_driven().with_lines(["maybe", "yes"]),
            PromptMode::Interactive,
            false,
        );
        assert!(
            prompter
                .confirm("Proceed?", false)
                .expect("yes after retry")
        );
        assert!(prompter.terminal().output().contains("'y' or 'n'"));

        let err = line_prompter(
            ScriptedTerminal::line_driven(),
            PromptMode::Interactive,
            false,
        )
        .confirm("Proceed?", true)
        .expect_err("closed input must error");
        assert!(err.message().contains("input closed"));
    }

    // ── Select: extra key navigation and line branches ──────────────────

    #[test]
    fn key_select_home_end_and_up_navigate_and_ignore_unrelated_keys() {
        // Start on the recommended default (Rust, index 1): End→node(2), Home→go(0), Down→rust(1),
        // Up→go(0), Tab→rust(1), Left is ignored, Enter confirms Rust.
        let choice = line_prompter(
            ScriptedTerminal::key_driven().with_keys([
                Key::End,
                Key::Home,
                Key::Down,
                Key::Up,
                Key::Tab,
                Key::Left,
                Key::Enter,
            ]),
            PromptMode::Interactive,
            false,
        )
        .select("Ecosystem?", &plain_choices())
        .expect("navigation resolves");
        assert_eq!(choice, ChoiceId::new("rust"));
    }

    #[test]
    fn line_select_empty_without_default_requires_a_choice() {
        let mut prompter = line_prompter(
            ScriptedTerminal::line_driven().with_lines(["", "2"]),
            PromptMode::Interactive,
            false,
        );
        let choice = prompter
            .select("Ecosystem?", &no_default_choices())
            .expect("valid choice after the required notice");
        assert_eq!(choice, ChoiceId::new("rust"));
        assert!(
            prompter
                .terminal()
                .output()
                .contains("a choice is required")
        );
    }

    // ── Multi-select: extra key navigation and line branches ────────────

    #[test]
    fn key_multi_select_escape_cancels_and_navigation_wraps() {
        let err = line_prompter(
            ScriptedTerminal::key_driven().with_key(Key::Escape),
            PromptMode::Interactive,
            false,
        )
        .multi_select("Tasks?", &plain_choices())
        .expect_err("escape cancels");
        assert!(err.message().contains("cancelled"));

        // End→2, Home→0, Up→2, Tab→0, Left ignored, Space toggles Go on, Enter.
        let selected = line_prompter(
            ScriptedTerminal::key_driven().with_keys([
                Key::End,
                Key::Home,
                Key::Up,
                Key::Tab,
                Key::Left,
                Key::Space,
                Key::Enter,
            ]),
            PromptMode::Interactive,
            false,
        )
        .multi_select("Tasks?", &plain_choices())
        .expect("navigation and toggle resolve");
        assert_eq!(selected, vec![ChoiceId::new("go"), ChoiceId::new("rust")]);
    }

    #[test]
    fn line_multi_select_notice_none_hint_and_close() {
        // No recommended choice → the default hint reads `[none]`;
        // an invalid answer shows a notice before a valid comma list is accepted.
        let mut prompter = line_prompter(
            ScriptedTerminal::line_driven().with_lines(["x", "1,2"]),
            PromptMode::Interactive,
            false,
        );
        let selected = prompter
            .multi_select("Tasks?", &no_default_choices())
            .expect("valid list after the notice");
        assert_eq!(selected, vec![ChoiceId::new("go"), ChoiceId::new("rust")]);
        let out = prompter.terminal().output();
        assert!(out.contains("[none]"));
        assert!(out.contains("comma-separated"));

        let err = line_prompter(
            ScriptedTerminal::line_driven(),
            PromptMode::Interactive,
            false,
        )
        .multi_select("Tasks?", &no_default_choices())
        .expect_err("closed input must error");
        assert!(err.message().contains("input closed"));
    }

    // ── Text: required, validator-reason, and cancellation branches ─────

    #[test]
    fn key_text_required_then_validator_reason_then_value() {
        let reject_short = |value: &str| {
            if value.len() >= 2 {
                Ok(())
            } else {
                Err("too short".to_string())
            }
        };

        // Enter on an empty buffer with no default surfaces the required notice,
        // then a typed value is accepted.
        let value = line_prompter(
            ScriptedTerminal::key_driven().with_keys([
                Key::Enter,
                Key::Char('h'),
                Key::Char('i'),
                Key::Enter,
            ]),
            PromptMode::Interactive,
            false,
        )
        .text("Name?", None)
        .expect("value after the required notice");
        assert_eq!(value, "hi");

        // A rejected value shows the validator reason, then a valid value passes.
        let mut prompter = line_prompter(
            ScriptedTerminal::key_driven().with_keys([
                Key::Char('a'),
                Key::Enter,
                Key::Char('b'),
                Key::Enter,
            ]),
            PromptMode::Interactive,
            false,
        );
        let value = prompter
            .text_with("Name?", None, &reject_short)
            .expect("valid after the reason");
        assert_eq!(value, "ab");
        assert!(prompter.terminal().output().contains("too short"));
    }

    #[test]
    fn key_text_ignores_unrelated_keys_and_escape_cancels() {
        let err = line_prompter(
            ScriptedTerminal::key_driven().with_keys([Key::Left, Key::Escape]),
            PromptMode::Interactive,
            false,
        )
        .text("Name?", None)
        .expect_err("escape cancels");
        assert!(err.message().contains("cancelled"));
    }

    #[test]
    fn line_text_errors_when_input_closes() {
        let err = line_prompter(
            ScriptedTerminal::line_driven(),
            PromptMode::Interactive,
            false,
        )
        .text("Name?", None)
        .expect_err("closed input must error");
        assert!(err.message().contains("input closed"));
    }

    #[test]
    fn line_text_with_shows_validator_reason_before_accepting() {
        let reject_short = |value: &str| {
            if value.len() >= 2 {
                Ok(())
            } else {
                Err("too short".to_string())
            }
        };
        let mut prompter = line_prompter(
            ScriptedTerminal::line_driven().with_lines(["a", "ok"]),
            PromptMode::Interactive,
            false,
        );
        let value = prompter
            .text_with("Name?", None, &reject_short)
            .expect("valid after the reason");
        assert_eq!(value, "ok");
        assert!(prompter.terminal().output().contains("too short"));
    }

    // ── Construction & metadata accessors ───────────────────────────────

    #[test]
    fn from_env_builds_a_prompter_and_reports_its_mode() {
        // Under a test harness neither stream is a TTY,
        // so the environment prompter is non-interactive;
        // building it also exercises terminal selection over the process streams.
        let prompter = Prompter::from_env(ColorChoice::Never);
        assert_eq!(prompter.mode(), PromptMode::NonInteractive);
    }

    #[test]
    fn with_glyphs_overrides_symbols_and_preserves_mode() {
        let prompter = line_prompter(
            ScriptedTerminal::line_driven(),
            PromptMode::NonInteractive,
            false,
        )
        .with_glyphs(Glyphs::new(true));
        assert_eq!(prompter.mode(), PromptMode::NonInteractive);
    }
}
