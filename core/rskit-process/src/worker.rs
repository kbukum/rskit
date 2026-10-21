//! Bounded joining of blocking worker threads shared by the std-thread runners (`sync` and `persistent`).
//!
//! Both blocking runners spawn capture-reader and stdin-writer threads
//! and must reap them once the child exits.
//! A naive `JoinHandle::join()` blocks forever when a surviving descendant inherited
//! and still holds the pipe open, so this module bounds the join by a grace period,
//! detaching a straggler rather than hanging the runner.
//! The straggler's bounded capture buffer caps its memory
//! and it exits on its own once the pipe finally closes.

use std::thread;
use std::time::{Duration, Instant};

use crate::{AppError, AppResult, ErrorCode};

/// How often [`join_within`] re-checks whether the worker has finished while waiting out the grace period.
const POLL_INTERVAL: Duration = Duration::from_millis(5);

/// Wait up to `grace` for every worker to finish, without consuming the handles.
///
/// Returns `false` when a worker is still running at the deadline, which after the leader exited usually means a surviving descendant still holds the pipe open.
pub(crate) fn finish_within(
    handles: &[Option<&thread::JoinHandle<AppResult<()>>>],
    grace: Duration,
) -> bool {
    let deadline = Instant::now() + grace;
    loop {
        if handles.iter().flatten().all(|handle| handle.is_finished()) {
            return true;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return false;
        }
        thread::sleep(remaining.min(POLL_INTERVAL));
    }
}

/// Join `handle` within `grace`, surfacing worker errors and mapping panics.
///
/// Returns `Ok(())` when there is no handle or the worker finishes within `grace`.
/// A worker still running after `grace` is detached (its handle is dropped) rather than joined forever. Required byte delivery reports a timeout instead of claiming completion; capture-only workers preserve their bounded partial-capture behavior.
///
/// The wait polls [`JoinHandle::is_finished`] instead of spawning a watchdog thread that blocks on `join()`:
/// a detached straggler must not leave a second thread parked in `join()` behind it,
/// or repeated runs would accumulate idle joiner threads. Once `is_finished()` reports `true`,
/// the `join()` below returns without blocking.
///
/// [`JoinHandle::is_finished`]: std::thread::JoinHandle::is_finished
pub(crate) fn join_within(
    handle: Option<thread::JoinHandle<AppResult<()>>>,
    grace: Duration,
    require_delivery: bool,
) -> AppResult<()> {
    let Some(handle) = handle else {
        return Ok(());
    };
    let deadline = Instant::now() + grace;
    while !handle.is_finished() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            // Detach the straggler: dropping the handle leaves the worker running,
            // and its bounded capture buffer caps memory until it exits once the pipe finally closes.
            return if require_delivery {
                Err(AppError::new(
                    ErrorCode::Timeout,
                    "process output delivery did not complete within the post-exit grace period",
                )
                .with_cause(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "process output worker is still running",
                )))
            } else {
                Ok(())
            };
        }
        thread::sleep(remaining.min(POLL_INTERVAL));
    }
    match handle.join() {
        Ok(result) => result,
        Err(_panic) => Err(AppError::new(
            ErrorCode::Internal,
            "process worker thread panicked",
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    #[test]
    fn join_within_handles_absent_finished_failed_and_panicked_workers() {
        join_within(None, Duration::from_millis(10), false).unwrap();
        join_within(None, Duration::from_millis(10), true).unwrap();

        let ok = thread::spawn(|| Ok(()));
        join_within(Some(ok), Duration::from_secs(1), false).unwrap();

        let failed = thread::spawn(|| Err(AppError::new(ErrorCode::Internal, "worker failed")));
        assert_eq!(
            join_within(Some(failed), Duration::from_secs(1), false)
                .unwrap_err()
                .code(),
            ErrorCode::Internal
        );

        let panicked = thread::spawn(|| -> AppResult<()> { panic!("worker panic") });
        assert_eq!(
            join_within(Some(panicked), Duration::from_secs(1), false)
                .unwrap_err()
                .code(),
            ErrorCode::Internal
        );
    }

    #[test]
    fn join_within_detaches_a_worker_that_outlives_the_grace_period() {
        let (unblock_tx, unblock_rx) = mpsc::channel::<()>();
        let slow = thread::spawn(move || {
            let _ = unblock_rx.recv();
            Ok(())
        });
        // The worker is still blocked,
        // so the bounded join returns without error rather than hanging.
        join_within(Some(slow), Duration::from_millis(20), false).unwrap();
        let _ = unblock_tx.send(());
    }
}
