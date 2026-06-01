use std::{
    sync::{Arc, atomic::AtomicBool, atomic::Ordering},
    thread,
    time::Duration,
};

use tokio_util::sync::CancellationToken;

use crate::{
    AppError, AppResult, ErrorCode, SignalPolicy,
    process_group::{kill_target, terminate_target},
};

#[derive(Debug)]
pub(in crate::persistent) struct CancelThread {
    stop: CancellationToken,
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
        self.stop.cancel();
        self.thread
            .join()
            .map_err(|_| AppError::new(ErrorCode::Internal, "process cancel thread panicked"))?
    }
}

pub(in crate::persistent) fn spawn_cancel_thread(
    pid: u32,
    cancel: CancellationToken,
    cancelled: Arc<AtomicBool>,
    signal: SignalPolicy,
    grace_period: Duration,
) -> AppResult<CancelThread> {
    let stop = CancellationToken::new();
    let wait_stop = stop.clone();
    let wait_cancel = cancel.clone();
    let wait_cancelled = Arc::clone(&cancelled);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                "failed to create process cancel runtime",
            )
            .with_cause(error)
        })?;
    let thread = thread::spawn(move || {
        runtime.block_on(async move {
            tokio::select! {
                biased;
                () = wait_cancel.cancelled() => {
                    wait_cancelled.store(true, Ordering::SeqCst);
                    let terminate_group = signal.create_process_group && signal.terminate_descendants;
                    let _ = terminate_target(pid, terminate_group);
                    tokio::select! {
                        () = wait_stop.cancelled() => Ok(()),
                        () = tokio::time::sleep(grace_period) => {
                            let _ = kill_target(pid, terminate_group);
                            Ok(())
                        }
                    }
                }
                () = wait_stop.cancelled() => Ok(()),
            }
        })
    });
    Ok(CancelThread {
        stop,
        cancel,
        cancelled,
        thread,
    })
}
