use tokio::{process::Child, time::timeout};
use tokio_util::sync::CancellationToken;
use tracing::debug;

use crate::supervisor::terminate_and_wait_async;
use crate::{AppError, AppResult, ErrorCode, ProcessConfig, ProcessSpec};

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
                let (exit_code, stderr) = terminate_and_wait_async(child, pid, config.lifecycle, "cancellation").await;
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
                        let (exit_code, stderr) = terminate_and_wait_async(child, pid, config.lifecycle, "timeout").await;
                        (exit_code, true, false, stderr)
                    }
                }
            }
        }
    } else {
        tokio::select! {
            _ = cancel.cancelled() => {
                debug!(program = %spec.program.display(), "process cancelled, sending SIGTERM");
                let (exit_code, stderr) = terminate_and_wait_async(child, pid, config.lifecycle, "cancellation").await;
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
