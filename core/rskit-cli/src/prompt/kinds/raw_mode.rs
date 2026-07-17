use crate::prompt::terminal::Terminal;

/// Run `body` between [`Terminal::begin_interactive`] and
/// [`Terminal::end_interactive`], restoring cooked mode on every exit path —
/// normal return, error, or an unwinding panic — via an RAII guard. On the
/// normal path teardown runs explicitly so `body`'s error is preferred over a
/// teardown error; if `body` panics the guard runs teardown during unwind so
/// the terminal is never left in raw mode.
pub(crate) fn with_raw_mode<T, R>(
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
