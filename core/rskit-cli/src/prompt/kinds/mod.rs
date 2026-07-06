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
    /// Run teardown on the normal path, returning its result so callers can
    /// surface it. Only disarms the `Drop` net when teardown *succeeds*, so a
    /// failed teardown leaves the guard armed and `Drop` retries as a
    /// best-effort net rather than leaving the terminal stuck in raw mode.
    fn disarm(&mut self) -> rskit_errors::AppResult<()> {
        let result = self.terminal.end_interactive();
        if result.is_ok() {
            self.armed = false;
        }
        result
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

/// Write the inline answer marker (`» [hint]: `, ASCII-safe via
/// [`Glyphs`](crate::theme::Glyphs)) and flush, for line prompts.
fn write_answer(
    terminal: &mut (impl Terminal + ?Sized),
    style: Style,
    hint: Option<&str>,
) -> rskit_errors::AppResult<()> {
    let marker = style.glyphs().answer();
    let text = hint.map_or_else(
        || format!("  {marker} "),
        |hint| format!("  {marker} {hint}: "),
    );
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
    use std::cell::Cell;

    use rskit_errors::{AppError, AppResult};

    use super::{focus_down, focus_up, with_raw_mode};
    use crate::prompt::terminal::{Capabilities, ScriptedTerminal, Terminal};

    /// A terminal whose `end_interactive` fails a fixed number of times before
    /// succeeding, and that counts every teardown attempt, so tests can prove
    /// the `Drop` net retries after an explicit teardown fails.
    struct FlakyTeardown {
        fail_remaining: Cell<u32>,
        teardown_calls: Cell<u32>,
        raw: Cell<bool>,
    }

    impl FlakyTeardown {
        fn failing(times: u32) -> Self {
            Self {
                fail_remaining: Cell::new(times),
                teardown_calls: Cell::new(0),
                raw: Cell::new(false),
            }
        }
    }

    impl Terminal for FlakyTeardown {
        fn capabilities(&self) -> Capabilities {
            Capabilities::key_driven()
        }
        fn read_line(&mut self) -> AppResult<Option<String>> {
            Ok(None)
        }
        fn read_key(&mut self) -> AppResult<crate::prompt::Key> {
            Err(AppError::invalid_input("terminal", "no keys scripted"))
        }
        fn write(&mut self, _text: &str) -> AppResult<()> {
            Ok(())
        }
        fn write_line(&mut self, _text: &str) -> AppResult<()> {
            Ok(())
        }
        fn flush(&mut self) -> AppResult<()> {
            Ok(())
        }
        fn clear_last_lines(&mut self, _count: u16) -> AppResult<()> {
            Ok(())
        }
        fn begin_interactive(&mut self) -> AppResult<()> {
            self.raw.set(true);
            Ok(())
        }
        fn end_interactive(&mut self) -> AppResult<()> {
            self.teardown_calls.set(self.teardown_calls.get() + 1);
            let remaining = self.fail_remaining.get();
            if remaining > 0 {
                self.fail_remaining.set(remaining - 1);
                return Err(AppError::invalid_input("terminal", "teardown failed"));
            }
            self.raw.set(false);
            Ok(())
        }
    }

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

    #[test]
    fn failed_teardown_is_retried_by_drop_net() {
        // The explicit teardown on the normal path fails once; the guard must
        // stay armed so Drop retries, ultimately restoring cooked mode.
        let mut term = FlakyTeardown::failing(1);
        let out = with_raw_mode(&mut term, |_| Ok(()));
        // The failed explicit teardown surfaces on the Ok path.
        assert!(out.is_err());
        // Called twice: the failing explicit attempt plus the successful retry.
        assert_eq!(term.teardown_calls.get(), 2);
        assert!(!term.raw.get());
    }
}
