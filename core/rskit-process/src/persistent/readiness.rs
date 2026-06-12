use std::{
    process::{Child, ExitStatus},
    sync::{atomic::AtomicBool, atomic::Ordering, mpsc},
    thread,
    time::{Duration, Instant},
};

use tokio_util::sync::CancellationToken;

use crate::{AppError, AppResult, ErrorCode, ProcessConfig, ProcessSpec, runner};

use super::{
    config::PersistentReadiness,
    error::{PersistentStartErrorKind, persistent_start_error},
};

pub(in crate::persistent) fn validate_readiness(readiness: &PersistentReadiness) -> AppResult<()> {
    if let PersistentReadiness::OutputContains(value) = readiness
        && value.is_empty()
    {
        return Err(AppError::invalid_input(
            "readiness.output",
            "output readiness marker must not be empty",
        ));
    }
    Ok(())
}

pub(in crate::persistent) fn readiness_wait_error(
    child: &mut Child,
    error: mpsc::RecvTimeoutError,
    cancelled: &AtomicBool,
) -> AppResult<AppError> {
    if let Some(status) = child.try_wait().map_err(AppError::internal)? {
        if cancelled.load(Ordering::SeqCst) {
            return Ok(AppError::cancelled("persistent process startup"));
        }
        return Ok(unexpected_exit_error(status));
    }
    if cancelled.load(Ordering::SeqCst) {
        return Ok(AppError::cancelled("persistent process startup"));
    }
    match error {
        mpsc::RecvTimeoutError::Timeout => Ok(persistent_start_error(
            PersistentStartErrorKind::ReadinessTimedOut,
            ErrorCode::Timeout,
            "persistent process did not become ready",
        )),
        mpsc::RecvTimeoutError::Disconnected => Ok(persistent_start_error(
            PersistentStartErrorKind::OutputEndedBeforeReadiness,
            ErrorCode::Internal,
            "persistent process output ended before readiness was observed",
        )),
    }
}

pub(in crate::persistent) fn wait_for_readiness(
    ready_rx: &mpsc::Receiver<()>,
    timeout: Duration,
    cancel: &CancellationToken,
    cancelled: &AtomicBool,
) -> Result<(), mpsc::RecvTimeoutError> {
    const CANCEL_POLL_INTERVAL: Duration = Duration::from_millis(10);
    let deadline = Instant::now() + timeout;

    loop {
        match ready_rx.try_recv() {
            Ok(()) => return Ok(()),
            Err(mpsc::TryRecvError::Disconnected) => {
                return Err(mpsc::RecvTimeoutError::Disconnected);
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }

        if cancel.is_cancelled() {
            cancelled.store(true, Ordering::SeqCst);
            return Err(mpsc::RecvTimeoutError::Timeout);
        }

        let now = Instant::now();
        if now >= deadline {
            return Err(mpsc::RecvTimeoutError::Timeout);
        }
        let wait = (deadline - now).min(CANCEL_POLL_INTERVAL);
        match ready_rx.recv_timeout(wait) {
            Ok(()) => return Ok(()),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(mpsc::RecvTimeoutError::Disconnected);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

pub(in crate::persistent) fn run_readiness_command(
    spec: &ProcessSpec,
    process_config: &ProcessConfig,
    timeout: Duration,
    shutdown_grace_period: Duration,
    cancel: CancellationToken,
) -> AppResult<()> {
    let mut config = process_config.clone();
    config.timeout = Some(timeout);
    config.signal = config.signal.with_grace_period(shutdown_grace_period);
    let spec = spec.clone();
    let result = thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("failed to create readiness command runtime: {error}"),
                )
            })?;
        runtime.block_on(runner::run_with_cancel(&spec, &config, cancel))
    })
    .join()
    .map_err(|_| AppError::new(ErrorCode::Internal, "readiness command runner panicked"))??;
    if result.timed_out {
        return Err(persistent_start_error(
            PersistentStartErrorKind::ReadinessCommandTimedOut,
            ErrorCode::Timeout,
            "persistent process readiness command timed out",
        ));
    }
    if result.cancelled {
        return Err(AppError::cancelled("persistent process readiness command"));
    }
    if result.success() {
        return Ok(());
    }
    Err(persistent_start_error(
        PersistentStartErrorKind::ReadinessCommandFailed,
        ErrorCode::Internal,
        "persistent process readiness command failed",
    ))
}

fn unexpected_exit_error(status: ExitStatus) -> AppError {
    persistent_start_error(
        PersistentStartErrorKind::ExitedBeforeReadiness,
        ErrorCode::Internal,
        format!("persistent process exited unexpectedly with status {status}"),
    )
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{atomic::Ordering, mpsc},
        time::{Duration, Instant},
    };

    use super::*;

    #[test]
    fn wait_for_readiness_returns_promptly_when_cancelled() {
        let (_ready_tx, ready_rx) = mpsc::channel();
        let cancel = CancellationToken::new();
        let cancelled = AtomicBool::new(false);
        cancel.cancel();
        let start = Instant::now();

        let error = wait_for_readiness(&ready_rx, Duration::from_secs(10), &cancel, &cancelled)
            .expect_err("cancelled readiness wait should fail");

        assert_eq!(error, mpsc::RecvTimeoutError::Timeout);
        assert!(cancelled.load(Ordering::SeqCst));
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn validate_readiness_rejects_empty_output_marker() {
        assert!(validate_readiness(&PersistentReadiness::Started).is_ok());
        assert!(validate_readiness(&PersistentReadiness::OutputContains("ready".into())).is_ok());
        assert!(validate_readiness(&PersistentReadiness::OutputContains(String::new())).is_err());
    }

    #[test]
    fn readiness_command_reports_failure_and_timeout_kinds() {
        let config = ProcessConfig::default().with_signal_policy(
            crate::SignalPolicy::default().with_grace_period(Duration::from_millis(10)),
        );
        let failed = run_readiness_command(
            &ProcessSpec::new("python3").args(["-c", "import sys; sys.exit(7)"]),
            &config,
            Duration::from_secs(2),
            Duration::from_millis(10),
            CancellationToken::new(),
        )
        .unwrap_err();
        assert_eq!(
            crate::persistent_start_error_kind(&failed),
            Some(PersistentStartErrorKind::ReadinessCommandFailed)
        );

        let timed_out = run_readiness_command(
            &ProcessSpec::new("python3").args(["-c", "import time; time.sleep(5)"]),
            &config,
            Duration::from_millis(20),
            Duration::from_millis(10),
            CancellationToken::new(),
        )
        .unwrap_err();
        assert_eq!(
            crate::persistent_start_error_kind(&timed_out),
            Some(PersistentStartErrorKind::ReadinessCommandTimedOut)
        );
    }

    #[test]
    fn wait_for_readiness_reports_disconnected_channel() {
        let (ready_tx, ready_rx) = mpsc::channel::<()>();
        drop(ready_tx);
        let cancel = CancellationToken::new();
        let cancelled = AtomicBool::new(false);

        let error = wait_for_readiness(&ready_rx, Duration::from_secs(1), &cancel, &cancelled)
            .expect_err("disconnected readiness channel should fail");

        assert_eq!(error, mpsc::RecvTimeoutError::Disconnected);
        assert!(!cancelled.load(Ordering::SeqCst));
    }

    #[test]
    fn readiness_wait_error_reports_cancelled_for_running_child() {
        let mut child = std::process::Command::new("sh")
            .arg("-c")
            .arg("sleep 2")
            .spawn()
            .expect("child process starts");
        let cancelled = AtomicBool::new(true);

        let error = readiness_wait_error(&mut child, mpsc::RecvTimeoutError::Timeout, &cancelled)
            .expect("readiness wait error maps to app error");

        assert_eq!(error.code(), ErrorCode::Cancelled);
    }
}
