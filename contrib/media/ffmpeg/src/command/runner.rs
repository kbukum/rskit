//! Process execution for compiled FFmpeg commands.
//!
//! Handles spawning the `ffmpeg` process, stderr streaming for progress
//! and error diagnostics, timeout enforcement, and process-group cleanup.

use std::sync::Arc;

use parking_lot::Mutex;
use rskit_media::pipeline::Progress;

use crate::config::FfmpegConfig;
use crate::process::run_ffmpeg_observed;
use crate::progress::FfmpegProgressParser;
use tokio_util::sync::CancellationToken;

use super::FfmpegCommand;

impl FfmpegCommand {
    /// Run the compiled FFmpeg command.
    ///
    /// Features:
    /// - Process group isolation (setpgid) for clean cleanup on Unix
    /// - Timeout enforcement via `tokio::time::timeout`
    /// - Streaming stderr collection for both progress parsing and error diagnostics
    /// - Progress reporting via `on_progress` callback (using mpsc channel)
    /// - Full stderr included in error messages on failure
    pub async fn run(
        &self,
        config: &FfmpegConfig,
        on_progress: Option<Box<dyn Fn(Progress) + Send + Sync>>,
        output_path: &std::path::Path,
    ) -> Result<(), crate::error::FfmpegError> {
        self.run_with_cancel(config, on_progress, output_path, CancellationToken::new())
            .await
    }

    /// Run the compiled FFmpeg command with cancellation support.
    pub async fn run_with_cancel(
        &self,
        config: &FfmpegConfig,
        on_progress: Option<Box<dyn Fn(Progress) + Send + Sync>>,
        output_path: &std::path::Path,
        cancel: CancellationToken,
    ) -> Result<(), crate::error::FfmpegError> {
        let mut args = self.to_args();
        args.push(output_path.to_string_lossy().to_string());

        tracing::debug!(cmd = %format!("ffmpeg {}", args.join(" ")), "executing ffmpeg");

        let stderr_lines = Arc::new(Mutex::new(Vec::new()));
        let progress_callback = on_progress.map(Arc::new);
        let result = run_ffmpeg_observed(config.ffmpeg_bin(), args, config.timeout, cancel, {
            let stderr_lines = Arc::clone(&stderr_lines);
            move |line| {
                if let Some(ref cb) = progress_callback {
                    let parser = FfmpegProgressParser::new(None);
                    if let Some(progress) = parser.parse_line(line) {
                        cb(progress);
                    }
                }
                stderr_lines.lock().push(line.to_string());
            }
        })
        .await
        .map_err(|error| crate::error::FfmpegError {
            kind: match error.code() {
                rskit_errors::ErrorCode::Timeout => crate::error::FfmpegErrorKind::Timeout,
                rskit_errors::ErrorCode::Cancelled => crate::error::FfmpegErrorKind::Cancelled,
                _ => crate::error::FfmpegErrorKind::SpawnFailed,
            },
            exit_code: None,
            stderr: String::new(),
            message: format!("ffmpeg execution failed: {error}"),
        })?;

        let stderr_output = stderr_lines.lock().join("\n");

        if result.timed_out {
            return Err(crate::error::FfmpegError {
                kind: crate::error::FfmpegErrorKind::Timeout,
                exit_code: result.exit_code,
                stderr: crate::error::truncate_stderr(&stderr_output, config.max_stderr_lines),
                message: format!("ffmpeg timed out after {:?}", config.timeout),
            });
        }

        if !result.success() {
            let exit_code = result.exit_code;
            let truncated_stderr =
                crate::error::truncate_stderr(&stderr_output, config.max_stderr_lines);
            let kind = crate::error::classify_error(exit_code, &stderr_output);

            let message = format!(
                "ffmpeg exited with status: {:?} (classified: {:?})",
                exit_code, kind
            );

            let err = crate::error::FfmpegError {
                kind,
                exit_code,
                stderr: truncated_stderr,
                message,
            };

            return Err(err);
        }

        Ok(())
    }
}
