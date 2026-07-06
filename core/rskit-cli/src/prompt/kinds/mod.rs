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
/// [`Terminal::end_interactive`], restoring cooked mode on every exit path —
/// normal return, error, or an unwinding panic — via an RAII guard. On the
/// normal path teardown runs explicitly so `body`'s error is preferred over a
/// teardown error; if `body` panics the guard runs teardown during unwind so
/// the terminal is never left in raw mode.
fn with_raw_mode<T, R>(
    terminal: &mut T,
    body: impl FnOnce(&mut T) -> rskit_errors::AppResult<R>,
) -> rskit_errors::AppResult<R>
where
    T: Terminal + ?Sized,
{
    terminal.begin_interactive()?;
    let mut guard = RawModeGuard {
        terminal,
        armed: true,
    };
    let result = body(&mut *guard.terminal);
    // Disarm and capture teardown so its error can be surfaced on the Ok path;
    // if `body` panicked instead, the guard's Drop restores cooked mode.
    let teardown = guard.disarm();
    match result {
        Ok(value) => teardown.map(|()| value),
        Err(error) => Err(error),
    }
}

/// RAII guard that restores cooked mode on drop unless explicitly disarmed.
struct RawModeGuard<'a, T: Terminal + ?Sized> {
    terminal: &'a mut T,
    armed: bool,
}

impl<T: Terminal + ?Sized> RawModeGuard<'_, T> {
    /// Run teardown once on the normal path and stop the `Drop` net from
    /// repeating it, returning the teardown result so callers can surface it.
    fn disarm(&mut self) -> rskit_errors::AppResult<()> {
        self.armed = false;
        self.terminal.end_interactive()
    }
}

impl<T: Terminal + ?Sized> Drop for RawModeGuard<'_, T> {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.terminal.end_interactive();
        }
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
    use super::{focus_down, focus_up, with_raw_mode};
    use crate::prompt::terminal::ScriptedTerminal;

    #[test]
    fn focus_wraps_both_ends() {
        assert_eq!(focus_up(0, 3), 2);
        assert_eq!(focus_up(2, 3), 1);
        assert_eq!(focus_down(2, 3), 0);
        assert_eq!(focus_down(0, 3), 1);
    }

    #[test]
    fn raw_mode_restored_on_normal_return() {
        let mut term = ScriptedTerminal::key_driven();
        let out = with_raw_mode(&mut term, |t| {
            assert!(t.is_interactive());
            Ok(7)
        });
        assert_eq!(out.expect("ok"), 7);
        assert!(!term.is_interactive());
    }

    #[test]
    fn raw_mode_restored_when_body_panics() {
        // A caller may catch the unwind and keep the terminal alive; the RAII
        // guard must still have run end_interactive() during unwinding so the
        // terminal is not stranded in raw mode.
        let mut term = ScriptedTerminal::key_driven();
        let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_raw_mode(&mut term, |_| -> rskit_errors::AppResult<()> {
                panic!("body blew up mid-prompt");
            })
        }));
        assert!(unwound.is_err());
        assert!(!term.is_interactive());
    }
}
