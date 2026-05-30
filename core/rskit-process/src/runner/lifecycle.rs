use tokio::{process::Child, time::timeout};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::{AppError, AppResult, Command, ErrorCode, ProcessConfig, signal::ProcessSignal};

pub(in crate::runner) struct Completion {
    pub(in crate::runner) exit_code: Option<i32>,
    pub(in crate::runner) timed_out: bool,
    pub(in crate::runner) cancelled: bool,
    pub(in crate::runner) synthetic_stderr: Option<String>,
}

pub(in crate::runner) async fn wait_for_completion(
    child: &mut Child,
    command: &Command,
    config: &ProcessConfig,
    cancel: CancellationToken,
) -> AppResult<Completion> {
    let pid = child.id();
    let (exit_code, timed_out, cancelled, synthetic_stderr) = if let Some(timeout_duration) =
        config.timeout
    {
        tokio::select! {
            _ = cancel.cancelled() => {
                debug!(program = %command.program.display(), "process cancelled, sending SIGTERM");
                let (exit_code, stderr) = terminate_and_wait(child, pid, config.grace_period, "cancellation").await;
                (exit_code, false, true, stderr)
            }
            wait_result = timeout(timeout_duration, child.wait()) => {
                match wait_result {
                    Ok(Ok(status)) => (status.code(), false, false, None),
                    Ok(Err(error)) => {
                        return Err(AppError::new(
                            ErrorCode::Internal,
                            format!("process execution error: {error}"),
                        ));
                    }
                    Err(_) => {
                        debug!(program = %command.program.display(), timeout = ?timeout_duration, "process timeout, sending SIGTERM");
                        let (exit_code, stderr) = terminate_and_wait(child, pid, config.grace_period, "timeout").await;
                        (exit_code, true, false, stderr)
                    }
                }
            }
        }
    } else {
        tokio::select! {
            _ = cancel.cancelled() => {
                debug!(program = %command.program.display(), "process cancelled, sending SIGTERM");
                let (exit_code, stderr) = terminate_and_wait(child, pid, config.grace_period, "cancellation").await;
                (exit_code, false, true, stderr)
            }
            wait_result = child.wait() => {
                match wait_result {
                    Ok(status) => (status.code(), false, false, None),
                    Err(error) => {
                        return Err(AppError::new(
                            ErrorCode::Internal,
                            format!("process execution error: {error}"),
                        ));
                    }
                }
            }
        }
    };

    Ok(Completion {
        exit_code,
        timed_out,
        cancelled,
        synthetic_stderr,
    })
}

async fn terminate_and_wait(
    child: &mut Child,
    pid: Option<u32>,
    grace_period: std::time::Duration,
    reason: &str,
) -> (Option<i32>, Option<String>) {
    if !terminate_process_group(pid, ProcessSignal::Terminate) {
        let _ = child.start_kill();
    }
    match timeout(grace_period, child.wait()).await {
        Ok(Ok(status)) => (status.code(), None),
        Ok(Err(error)) => {
            warn!(
                signal = ProcessSignal::Terminate.name(),
                "error waiting for process after signal: {error}"
            );
            if !terminate_process_group(pid, ProcessSignal::Kill) {
                let _ = child.start_kill();
            }
            (
                None,
                Some(format!(
                    "process killed (error during grace period after {reason}: {error})"
                )),
            )
        }
        Err(_) => {
            debug!(
                signal = ProcessSignal::Kill.name(),
                "grace period expired, sending signal"
            );
            if !terminate_process_group(pid, ProcessSignal::Kill) {
                let _ = child.start_kill();
            }
            let _ = child.wait().await;
            (
                None,
                Some(format!("process killed by SIGKILL after {reason}")),
            )
        }
    }
}

fn terminate_process_group(pid: Option<u32>, signal: ProcessSignal) -> bool {
    if let Some(pid) = pid {
        #[cfg(unix)]
        // SAFETY: `kill` is invoked with the negated process-group id created by
        // the `pre_exec` hook so signals fan out to the subprocess tree.
        // Errors are handled explicitly and ignored only for `ESRCH`.
        unsafe {
            let result = libc::kill(-(pid as i32), signal.as_raw());
            if result != 0 {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::ESRCH) {
                    warn!(signal = signal.name(), "failed to send signal: {error}");
                    return false;
                }
            }
            return true;
        }
    }
    false
}
