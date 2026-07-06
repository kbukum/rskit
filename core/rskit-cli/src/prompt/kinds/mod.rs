//! The prompt kinds: one behavioural module per question type.
//!
//! Each kind implements the same question three ways from one place:
//!
//! - **non-interactive** — resolve to the declared default or return a typed
//!   error, without touching the terminal;
//! - **line-driven** — print a numbered list (or plain prompt) once and parse a
//!   typed answer, re-asking on invalid input;
//! - **key-driven** — draw a live frame and update it in place as arrow keys
//!   move focus and space toggles selection.
//!
//! A kind chooses the interactive path by inspecting
//! [`Terminal::capabilities`], so the
//! prompter never branches on a concrete terminal type.

pub mod confirm;
pub mod multi_select;
pub mod select;
pub mod text;

use rskit_errors::AppError;

use super::render::Style;
use super::terminal::Terminal;

/// The error returned when a non-interactive prompt has no usable default.
fn non_interactive_error(prompt: &str) -> AppError {
    AppError::invalid_input(
        "prompt",
        format!("non-interactive mode requires a default for: {prompt}"),
    )
}

/// The error returned when input closes before the prompt is answered.
fn closed_input(prompt: &str) -> AppError {
    AppError::invalid_input("prompt", format!("input closed before answering: {prompt}"))
}

/// The error returned when the user cancels an interactive prompt (Esc/Ctrl+C).
fn cancelled(prompt: &str) -> AppError {
    AppError::cancelled(format!("prompt cancelled: {prompt}"))
}

/// Run `body` between [`Terminal::begin_interactive`] and
/// [`Terminal::end_interactive`], attempting to restore cooked mode even when
/// `body` fails, and preferring `body`'s error over a teardown error. If
/// teardown itself errors the restoration is best-effort; `RichTerminal`'s
/// `Drop` net is the final guarantee that raw mode is disabled.
fn with_raw_mode<T, R>(
    terminal: &mut T,
    body: impl FnOnce(&mut T) -> rskit_errors::AppResult<R>,
) -> rskit_errors::AppResult<R>
where
    T: Terminal + ?Sized,
{
    terminal.begin_interactive()?;
    let result = body(terminal);
    let teardown = terminal.end_interactive();
    match result {
        Ok(value) => teardown.map(|()| value),
        Err(error) => Err(error),
    }
}

/// Step focus up by one, wrapping to the last row.
const fn focus_up(cursor: usize, len: usize) -> usize {
    if cursor == 0 { len - 1 } else { cursor - 1 }
}

/// Step focus down by one, wrapping to the first row.
const fn focus_down(cursor: usize, len: usize) -> usize {
    if cursor + 1 >= len { 0 } else { cursor + 1 }
}

/// Write the inline answer marker (`» [hint]: `) and flush, for line prompts.
fn write_answer(
    terminal: &mut (impl Terminal + ?Sized),
    hint: Option<&str>,
) -> rskit_errors::AppResult<()> {
    let text = hint.map_or_else(|| "  » ".to_string(), |hint| format!("  » {hint}: "));
    terminal.write(&text)?;
    terminal.flush()
}

/// Write a dimmed warning notice line beneath a line prompt.
fn notice(
    terminal: &mut (impl Terminal + ?Sized),
    style: Style,
    text: &str,
) -> rskit_errors::AppResult<()> {
    let styled = style.palette().warn(text).into_owned();
    terminal.write_line(&format!("  {styled}"))
}

/// Parse a one-based choice number into a zero-based index within `0..len`.
fn parse_index(input: &str, len: usize) -> Option<usize> {
    let number: usize = input.parse().ok()?;
    (1..=len).contains(&number).then(|| number - 1)
}

#[cfg(test)]
mod tests {
    use super::{focus_down, focus_up};

    #[test]
    fn focus_wraps_both_ends() {
        assert_eq!(focus_up(0, 3), 2);
        assert_eq!(focus_up(2, 3), 1);
        assert_eq!(focus_down(2, 3), 0);
        assert_eq!(focus_down(0, 3), 1);
    }
}
