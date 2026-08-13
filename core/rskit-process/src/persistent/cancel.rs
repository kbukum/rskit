use std::{
    sync::{Arc, atomic::AtomicBool, atomic::Ordering},
    thread,
    time::{Duration, Instant},
};

use tokio_util::sync::CancellationToken;

use crate::process_group::{kill_target, terminate_target};
use crate::{AppError, AppResult, ErrorCode, LifecyclePolicy};

const POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug)]
pub(in crate::persistent) struct CancelThread {
    stop: Arc<AtomicBool>,
    cancel: CancellationToken,
    cancelled: Arc<AtomicBool>,
    thread: thread::JoinHandle<AppResult<()>>,
}

impl CancelThread {
    pub(in crate::persistent) fn is_cancel_requested(&self) -> bool {
        self.cancel.is_cancelled() || self.cancelled.load(Ordering::SeqCst)
    }

    pub(in crate::persistent) fn stop(self) -> AppResult<()> {
        if self.is_cancel_requested() {
            self.cancelled.store(true, Ordering::SeqCst);
        }
        self.stop.store(true, Ordering::SeqCst);
        self.thread
            .join()
            .map_err(|_| AppError::new(ErrorCode::Internal, "process cancel thread panicked"))?
    }
}

/// Spawn a plain OS thread that watches the caller's cancellation token and, on cancellation,
/// escalates `SIGTERM` → grace → `SIGKILL` against the child.
///
/// This is a std-thread poll loop rather than a per-process Tokio runtime:
/// the only asynchronous input is the caller's [`CancellationToken`],
/// which exposes a non-blocking `is_cancelled()`,
/// so a lightweight poll avoids standing up a whole runtime (and its worker thread) for every persistent process.
pub(in crate::persistent) fn spawn_cancel_thread(
    pid: u32,
    cancel: CancellationToken,
    cancelled: Arc<AtomicBool>,
    signal: LifecyclePolicy,
    grace_period: Duration,
) -> AppResult<CancelThread> {
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);
    let thread_cancel = cancel.clone();
    let thread_cancelled = Arc::clone(&cancelled);
    let group = signal.targets_group();
    let thread = thread::spawn(move || {
        while !thread_stop.load(Ordering::SeqCst) {
            if thread_cancel.is_cancelled() {
                thread_cancelled.store(true, Ordering::SeqCst);
                let _ = terminate_target(pid, group);
                if signal.kill_after_grace {
                    escalate_after_grace(&thread_stop, pid, group, grace_period);
                }
                return Ok(());
            }
            thread::sleep(POLL_INTERVAL);
        }
        Ok(())
    });
    Ok(CancelThread {
        stop,
        cancel,
        cancelled,
        thread,
    })
}

/// Wait up to `grace_period` for a stop signal after `SIGTERM`;
/// escalate to `SIGKILL` if the owning thread has not reaped the child in time.
fn escalate_after_grace(stop: &AtomicBool, pid: u32, group: bool, grace_period: Duration) {
    let deadline = Instant::now() + grace_period;
    while !stop.load(Ordering::SeqCst) {
        if Instant::now() >= deadline {
            let _ = kill_target(pid, group);
            return;
        }
        thread::sleep(POLL_INTERVAL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_thread_records_requested_cancellation_before_stop() {
        let cancel = CancellationToken::new();
        let cancelled = Arc::new(AtomicBool::new(false));
        let thread = spawn_cancel_thread(
            0,
            cancel.clone(),
            Arc::clone(&cancelled),
            LifecyclePolicy::default(),
            Duration::from_millis(1),
        )
        .unwrap();

        cancel.cancel();
        while !thread.is_cancel_requested() {
            std::thread::sleep(Duration::from_millis(1));
        }

        assert!(thread.is_cancel_requested());
        thread.stop().unwrap();
        assert!(cancelled.load(Ordering::SeqCst));
    }
}
