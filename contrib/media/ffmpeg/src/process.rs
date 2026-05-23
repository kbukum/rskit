//! FFmpeg subprocess helpers backed by `rskit-process`.

use std::path::PathBuf;
use std::time::Duration;

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_process::{OutputObserver, ProcessConfig, ProcessResult, command as process_command};
use tokio_util::sync::CancellationToken;

pub(crate) async fn run_capture(
    program: PathBuf,
    args: impl IntoIterator<Item = String>,
    timeout: Option<Duration>,
) -> AppResult<ProcessResult> {
    run_capture_with_cancel(program, args, timeout, CancellationToken::new()).await
}

pub(crate) async fn run_capture_with_cancel(
    program: PathBuf,
    args: impl IntoIterator<Item = String>,
    timeout: Option<Duration>,
    cancel: CancellationToken,
) -> AppResult<ProcessResult> {
    let command = process_command(program_to_string(&program)?).args(args);
    let config = ProcessConfig {
        timeout,
        ..ProcessConfig::default()
    };
    rskit_process::run_with_cancel(&command, &config, cancel).await
}

pub(crate) async fn run_capture_lossy(
    program: PathBuf,
    args: impl IntoIterator<Item = impl AsRef<str>>,
    timeout: Option<Duration>,
) -> AppResult<ProcessResult> {
    run_capture(
        program,
        args.into_iter().map(|arg| arg.as_ref().to_string()),
        timeout,
    )
    .await
}

pub(crate) async fn run_ffmpeg_observed(
    program: PathBuf,
    args: Vec<String>,
    timeout: Option<Duration>,
    cancel: CancellationToken,
    stderr_line: impl Fn(&str) + Send + Sync + 'static,
) -> AppResult<ProcessResult> {
    let command = process_command(program_to_string(&program)?).args(args);
    let config = ProcessConfig {
        timeout,
        ..ProcessConfig::default()
    };
    rskit_process::run_with_observer(
        &command,
        &config,
        cancel,
        OutputObserver::new().with_stderr_line(stderr_line),
    )
    .await
}

fn program_to_string(program: &std::path::Path) -> AppResult<String> {
    program.to_str().map(ToOwned::to_owned).ok_or_else(|| {
        AppError::new(
            ErrorCode::InvalidInput,
            format!(
                "process program path is not valid UTF-8: {}",
                program.display()
            ),
        )
    })
}

pub(crate) fn ensure_success(result: &ProcessResult, context: &str) -> AppResult<()> {
    if result.success() {
        return Ok(());
    }
    if result.timed_out {
        return Err(AppError::new(
            ErrorCode::Timeout,
            format!("{context} timed out: {}", result.stderr),
        ));
    }
    Err(AppError::new(
        ErrorCode::Internal,
        format!("{context} failed: {}", result.stderr),
    ))
}
