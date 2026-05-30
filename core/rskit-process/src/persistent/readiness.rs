use std::{
    process::{Child, ExitStatus},
    sync::{atomic::AtomicBool, atomic::Ordering, mpsc},
    thread,
    time::Duration,
};

use tokio_util::sync::CancellationToken;

use crate::{AppError, AppResult, Command, ErrorCode, ProcessConfig, runner};

use super::config::PersistentReadiness;

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
        mpsc::RecvTimeoutError::Timeout => Ok(AppError::new(
            ErrorCode::Timeout,
            "persistent process did not become ready",
        )),
        mpsc::RecvTimeoutError::Disconnected => Ok(AppError::new(
            ErrorCode::Internal,
            "persistent process output ended before readiness was observed",
        )),
    }
}

pub(in crate::persistent) fn run_readiness_command(
    command: &Command,
    process_config: &ProcessConfig,
    timeout: Duration,
    cancel: CancellationToken,
) -> AppResult<()> {
    let mut config = process_config.clone();
    config.timeout = Some(timeout);
    let command = command.clone();
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
        runtime.block_on(runner::run_with_cancel(&command, &config, cancel))
    })
    .join()
    .map_err(|_| AppError::new(ErrorCode::Internal, "readiness command runner panicked"))??;
    if result.success() {
        return Ok(());
    }
    if result.timed_out {
        return Err(AppError::new(
            ErrorCode::Timeout,
            "persistent process readiness command timed out",
        ));
    }
    Err(AppError::new(
        ErrorCode::Internal,
        "persistent process readiness command failed",
    ))
}

fn unexpected_exit_error(status: ExitStatus) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("persistent process exited unexpectedly with status {status}"),
    )
}
