use tokio::{process::Child, time::timeout};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::process_group::{kill_target, terminate_target};
use crate::{
    AppError, AppResult, ErrorCode, ProcessConfig, ProcessSpec, signal::ProcessSignal, terminate,
};

pub(in crate::runner) struct Completion {
    pub(in crate::runner) exit_code: Option<i32>,
    pub(in crate::runner) timed_out: bool,
    pub(in crate::runner) cancelled: bool,
    pub(in crate::runner) synthetic_stderr: Option<String>,
}

pub(in crate::runner) async fn wait_for_completion(
    child: &mut Child,
    spec: &ProcessSpec,
    config: &ProcessConfig,
    cancel: CancellationToken,
) -> AppResult<Completion> {
    let pid = child.id();
    let (exit_code, timed_out, cancelled, synthetic_stderr) = if let Some(timeout_duration) =
        config.timeout
    {
        tokio::select! {
            _ = cancel.cancelled() => {
                debug!(program = %spec.program.display(), "process cancelled, sending SIGTERM");
                let (exit_code, stderr) = terminate_and_wait(child, pid, config, "cancellation").await;
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
                        debug!(program = %spec.program.display(), timeout = ?timeout_duration, "process timeout, sending SIGTERM");
                        let (exit_code, stderr) = terminate_and_wait(child, pid, config, "timeout").await;
                        (exit_code, true, false, stderr)
                    }
                }
            }
        }
    } else {
        tokio::select! {
            _ = cancel.cancelled() => {
                debug!(program = %spec.program.display(), "process cancelled, sending SIGTERM");
                let (exit_code, stderr) = terminate_and_wait(child, pid, config, "cancellation").await;
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
    config: &ProcessConfig,
    reason: &str,
) -> (Option<i32>, Option<String>) {
    let group = terminate::targets_group(config.signal);
    if !pid.is_some_and(|pid| terminate_target(pid, group)) {
        let _ = child.start_kill();
    }
    match timeout(config.signal.grace_period, child.wait()).await {
        Ok(Ok(status)) => (status.code(), None),
        Ok(Err(error)) => {
            warn!(
                signal = ProcessSignal::Terminate.name(),
                "error waiting for process after signal: {error}"
            );
            if !pid.is_some_and(|pid| kill_target(pid, group)) {
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
            if !pid.is_some_and(|pid| kill_target(pid, group)) {
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
