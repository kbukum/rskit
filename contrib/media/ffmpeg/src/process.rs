//! FFmpeg subprocess helpers backed by `rskit-process`.

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_process::{
    ObservedIo, OutputObserver, ProcessConfig, ProcessIo, ProcessResult, ProcessSpec,
};
use tokio_util::sync::CancellationToken;

const MAX_CAPTURE_BYTES: usize = 64 * 1024 * 1024;

pub(crate) async fn run_capture(
    program: PathBuf,
    args: impl IntoIterator<Item = OsString>,
    timeout: Option<Duration>,
) -> AppResult<ProcessResult> {
    run_capture_with_cancel(program, args, timeout, CancellationToken::new()).await
}

pub(crate) async fn run_capture_with_cancel(
    program: PathBuf,
    args: impl IntoIterator<Item = OsString>,
    timeout: Option<Duration>,
    cancel: CancellationToken,
) -> AppResult<ProcessResult> {
    let command = ProcessSpec::new(program).args(args);
    let config = ProcessConfig::default()
        .with_timeout(timeout)
        .with_max_output_bytes(MAX_CAPTURE_BYTES);
    let result = rskit_process::run_with_cancel(&command, &config, cancel).await?;
    if result.cancelled {
        return Err(AppError::new(ErrorCode::Cancelled, "process cancelled"));
    }
    if result.timed_out {
        return Ok(result);
    }
    if result.stdout_truncated || result.stderr_truncated {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            format!("process output reached capture limit of {MAX_CAPTURE_BYTES} bytes"),
        ));
    }
    Ok(result)
}

pub(crate) async fn run_capture_lossy_with_cancel(
    program: PathBuf,
    args: impl IntoIterator<Item = impl AsRef<str>>,
    timeout: Option<Duration>,
    cancel: CancellationToken,
) -> AppResult<ProcessResult> {
    run_capture_with_cancel(
        program,
        args.into_iter().map(|arg| OsString::from(arg.as_ref())),
        timeout,
        cancel,
    )
    .await
}

pub(crate) async fn run_ffmpeg_observed(
    program: PathBuf,
    args: Vec<OsString>,
    timeout: Option<Duration>,
    cancel: CancellationToken,
    stderr_line: impl Fn(&str) + Send + Sync + 'static,
) -> AppResult<ProcessResult> {
    let command = ProcessSpec::new(program).args(args);
    let config = ProcessConfig::default()
        .with_timeout(timeout)
        .with_io(ProcessIo::observed(ObservedIo::new(
            OutputObserver::new().with_stderr_line(stderr_line),
        )));
    rskit_process::run_with_cancel(&command, &config, cancel).await
}

pub(crate) fn with_context(error: AppError, context: impl std::fmt::Display) -> AppError {
    AppError::new(error.code(), format!("{context}: {error}"))
}

pub(crate) fn ensure_success(result: &ProcessResult, context: &str) -> AppResult<()> {
    if result.cancelled {
        return Err(AppError::new(
            ErrorCode::Cancelled,
            format!("{context} cancelled"),
        ));
    }
    if result.timed_out {
        return Err(AppError::new(
            ErrorCode::Timeout,
            format!("{context} timed out: {}", result.stderr),
        ));
    }
    if result.success() {
        return Ok(());
    }
    Err(AppError::new(
        ErrorCode::Internal,
        format!("{context} failed: {}", result.stderr),
    ))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use rskit_process::ProcessResult;

    use super::*;

    #[test]
    fn ensure_success_preserves_cancelled_result() {
        let result = ProcessResult::completed(
            None,
            Vec::new(),
            Vec::new(),
            false,
            false,
            Duration::from_millis(1),
            false,
            true,
        );

        let error = ensure_success(&result, "ffmpeg").expect_err("cancelled result fails");

        assert_eq!(error.code(), ErrorCode::Cancelled);
    }

    #[test]
    fn ensure_success_accepts_successful_process() {
        let result = ProcessResult::completed(
            Some(0),
            Vec::new(),
            Vec::new(),
            false,
            false,
            Duration::from_millis(1),
            false,
            false,
        );

        ensure_success(&result, "ffmpeg").unwrap();
    }

    #[test]
    fn ensure_success_maps_timeout_and_failure() {
        let timed_out = ProcessResult::completed(
            None,
            Vec::new(),
            b"slow".to_vec(),
            false,
            false,
            Duration::from_millis(1),
            true,
            false,
        );
        let timeout = ensure_success(&timed_out, "ffmpeg").unwrap_err();
        assert_eq!(timeout.code(), ErrorCode::Timeout);
        assert!(timeout.message().contains("slow"));

        let failed = ProcessResult::completed(
            Some(1),
            Vec::new(),
            b"bad input".to_vec(),
            false,
            false,
            Duration::from_millis(1),
            false,
            false,
        );
        let failure = ensure_success(&failed, "ffmpeg").unwrap_err();
        assert_eq!(failure.code(), ErrorCode::Internal);
        assert!(failure.message().contains("bad input"));
    }

    #[test]
    fn context_preserves_error_code_and_adds_operation() {
        let error = AppError::new(ErrorCode::InvalidInput, "bad input");

        let contextual = with_context(error, "ffmpeg probe");

        assert_eq!(contextual.code(), ErrorCode::InvalidInput);
        assert!(contextual.message().contains("ffmpeg probe"));
        assert!(contextual.message().contains("bad input"));
    }
}
