//! Bounded joining of blocking worker threads shared by the std-thread runners
//! (`sync` and `persistent`).
//!
//! Both blocking runners spawn capture-reader and stdin-writer threads and must
//! reap them once the child exits. A naive `JoinHandle::join()` blocks forever
//! when a surviving descendant inherited and still holds the pipe open, so this
//! module bounds the join by a grace period, detaching a straggler rather than
//! hanging the runner. The straggler's bounded capture buffer caps its memory
//! and it exits on its own once the pipe finally closes.

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::{AppError, AppResult, ErrorCode};

/// Join `handle` within `grace`, surfacing worker errors and mapping panics.
///
/// Returns `Ok(())` when there is no handle or the worker finishes within
/// `grace`. A worker still running after `grace` is detached via a watchdog
/// thread rather than joined forever.
pub(crate) fn join_within(
    handle: Option<thread::JoinHandle<AppResult<()>>>,
    grace: Duration,
) -> AppResult<()> {
    let Some(handle) = handle else {
        return Ok(());
    };
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(handle.join());
    });
    match rx.recv_timeout(grace) {
        Ok(Ok(result)) => result,
        Ok(Err(_panic)) => Err(AppError::new(
            ErrorCode::Internal,
            "process worker thread panicked",
        )),
        Err(_timeout) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_within_handles_absent_finished_failed_and_panicked_workers() {
        join_within(None, Duration::from_millis(10)).unwrap();

        let ok = thread::spawn(|| Ok(()));
        join_within(Some(ok), Duration::from_secs(1)).unwrap();

        let failed = thread::spawn(|| Err(AppError::new(ErrorCode::Internal, "worker failed")));
        assert_eq!(
            join_within(Some(failed), Duration::from_secs(1))
                .unwrap_err()
                .code(),
            ErrorCode::Internal
        );

        let panicked = thread::spawn(|| -> AppResult<()> { panic!("worker panic") });
        assert_eq!(
            join_within(Some(panicked), Duration::from_secs(1))
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
        // The worker is still blocked, so the bounded join returns without error
        // rather than hanging.
        join_within(Some(slow), Duration::from_millis(20)).unwrap();
        let _ = unblock_tx.send(());
    }
}
