//! Async subprocess execution runtime.

use std::time::Instant;

use tokio::process::Command as TokioCommand;
use tokio_util::sync::CancellationToken;
use tracing::debug;

use crate::{AppError, AppResult, Command, ErrorCode, ProcessConfig, ProcessResult};

mod lifecycle;
mod observer;
mod output;
mod spawn;

pub use observer::OutputObserver;

use lifecycle::wait_for_completion;
use output::{append_bounded_stderr, collect_reader, spawn_reader};
use spawn::configure_command;

/// Execute a subprocess with the given configuration and cancellation token.
pub async fn run_with_cancel(
    command: &Command,
    config: &ProcessConfig,
    cancel: CancellationToken,
) -> AppResult<ProcessResult> {
    run_process(command, config, cancel, None).await
}

/// Execute a subprocess and observe stdout/stderr lines as they are emitted.
pub async fn run_with_observer(
    command: &Command,
    config: &ProcessConfig,
    cancel: CancellationToken,
    observer: OutputObserver,
) -> AppResult<ProcessResult> {
    run_process(command, config, cancel, Some(observer)).await
}

async fn run_process(
    command: &Command,
    config: &ProcessConfig,
    cancel: CancellationToken,
    observer: Option<OutputObserver>,
) -> AppResult<ProcessResult> {
    if command.program.as_os_str().is_empty() {
        return Err(AppError::invalid_input("program", "must not be empty"));
    }

    let start = Instant::now();
    let stdout_observer = observer
        .as_ref()
        .and_then(|observer| observer.stdout_line.clone());
    let stderr_observer = observer
        .as_ref()
        .and_then(|observer| observer.stderr_line.clone());

    let mut cmd = TokioCommand::new(&command.program);
    configure_command(
        &mut cmd,
        command,
        config,
        stdout_observer.is_some(),
        stderr_observer.is_some(),
    );

    debug!(program = %command.program.display(), args = ?command.args, "spawning process");
    let mut child = cmd.spawn().map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to spawn process: {error}"),
        )
    })?;

    let stdout_task = spawn_reader(
        child.stdout.take(),
        config.max_output_bytes,
        stdout_observer,
        config.capture_output,
    );
    let stderr_task = spawn_reader(
        child.stderr.take(),
        config.max_output_bytes,
        stderr_observer,
        config.capture_output,
    );

    if let Some(stdin_data) = &command.stdin
        && let Some(mut stdin) = child.stdin.take()
    {
        use tokio::io::AsyncWriteExt;

        stdin.write_all(stdin_data).await.map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to write to stdin: {error}"),
            )
        })?;
    }

    let completion = wait_for_completion(&mut child, command, config, cancel).await?;

    let stdout_output = collect_reader(stdout_task).await?;
    let stdout_bytes = stdout_output.bytes;
    let stdout_truncated = stdout_output.truncated;
    let stderr_output = collect_reader(stderr_task).await?;
    let mut stderr_bytes = stderr_output.bytes;
    let mut stderr_truncated = stderr_output.truncated;
    if let Some(extra_stderr) = completion.synthetic_stderr {
        stderr_truncated |= append_bounded_stderr(
            &mut stderr_bytes,
            extra_stderr.as_bytes(),
            config.max_output_bytes,
        );
    }

    let result = ProcessResult::completed(
        completion.exit_code,
        stdout_bytes,
        stderr_bytes,
        stdout_truncated,
        stderr_truncated,
        start.elapsed(),
        completion.timed_out,
        completion.cancelled,
    );

    debug!(
        exit_code = ?result.exit_code,
        duration = ?result.duration,
        timed_out = result.timed_out,
        "process completed"
    );

    if result.cancelled {
        let mut error = AppError::new(ErrorCode::Cancelled, "process cancelled")
            .with_detail("timed_out", result.timed_out)
            .with_detail("duration_ms", result.duration.as_millis() as u64)
            .with_detail("stdout", result.stdout.clone())
            .with_detail("stderr", result.stderr.clone());
        if let Some(exit_code) = result.exit_code {
            error = error.with_detail("exit_code", exit_code);
        }
        return Err(error);
    }

    Ok(result)
}
