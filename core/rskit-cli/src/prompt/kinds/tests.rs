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
    let out = with_raw_mode(&mut term, |t| {
        // Exercise the full terminal surface so the double is covered end to
        // end: capabilities, both reads, both writes, flush, and clear.
        assert!(t.capabilities().is_key_driven());
        t.write("x")?;
        t.write_line("y")?;
        t.flush()?;
        t.clear_last_lines(1)?;
        assert_eq!(t.read_line()?, None);
        assert!(t.read_key().is_err());
        Ok(())
    });
    // The failed explicit teardown surfaces on the Ok path.
    assert!(out.is_err());
    // Called twice: the failing explicit attempt plus the successful retry.
    assert_eq!(term.teardown_calls.get(), 2);
    assert!(!term.raw.get());
}
