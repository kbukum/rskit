//! Process execution for compiled FFmpeg commands.
//!
//! Handles spawning the `ffmpeg` process, stderr streaming for progress
//! and error diagnostics, timeout enforcement, and process-group cleanup.

use std::collections::VecDeque;
use std::sync::Arc;

use parking_lot::Mutex;
use rskit_media::pipeline::Progress;

use crate::config::FfmpegConfig;
use crate::process::run_ffmpeg_observed;
use crate::progress::FfmpegProgressParser;
use tokio_util::sync::CancellationToken;

use super::FfmpegCommand;

impl FfmpegCommand {
    /// Run the compiled FFmpeg command with cancellation support.
    pub async fn run_with_cancel(
        &self,
        config: &FfmpegConfig,
        on_progress: Option<Box<dyn Fn(Progress) + Send + Sync>>,
        output_path: &std::path::Path,
        cancel: CancellationToken,
    ) -> Result<(), crate::error::FfmpegError> {
        let mut args = self.to_os_args();
        args.push(output_path.as_os_str().to_os_string());

        tracing::debug!(args = ?args, "executing ffmpeg");

        let max_stderr_lines = config.max_stderr_lines.max(1);
        let stderr_lines = Arc::new(Mutex::new(VecDeque::with_capacity(max_stderr_lines)));
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
                push_stderr_line(&stderr_lines, max_stderr_lines, line);
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

        let stderr_output = stderr_lines
            .lock()
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");

        if result.cancelled {
            return Err(crate::error::FfmpegError {
                kind: crate::error::FfmpegErrorKind::Cancelled,
                exit_code: result.exit_code,
                stderr: crate::error::truncate_stderr(&stderr_output, config.max_stderr_lines),
                message: "ffmpeg execution cancelled".to_string(),
            });
        }

        if result.timed_out {
            let timeout = config
                .timeout
                .map(|duration| format!("{duration:?}"))
                .unwrap_or_else(|| "configured timeout".to_string());
            return Err(crate::error::FfmpegError {
                kind: crate::error::FfmpegErrorKind::Timeout,
                exit_code: result.exit_code,
                stderr: crate::error::truncate_stderr(&stderr_output, config.max_stderr_lines),
                message: format!("ffmpeg timed out after {timeout}"),
            });
        }

        if !result.success() {
            let exit_code = result.exit_code;
            let truncated_stderr =
                crate::error::truncate_stderr(&stderr_output, config.max_stderr_lines);
            let classification_stderr = if result.stderr.is_empty() {
                stderr_output.clone()
            } else if stderr_output.is_empty() {
                result.stderr.clone()
            } else {
                format!("{}\n{}", result.stderr, stderr_output)
            };
            let kind = crate::error::classify_error(exit_code, &classification_stderr);

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

fn push_stderr_line(lines: &Mutex<VecDeque<String>>, max_stderr_lines: usize, line: &str) {
    let mut lines = lines.lock();
    if lines.len() == max_stderr_lines {
        lines.pop_front();
    }
    lines.push_back(line.to_string());
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use rskit_media::registry::Registry;
    use tokio_util::sync::CancellationToken;

    use super::*;

    #[cfg(unix)]
    use crate::test_support::write_executable_script as write_script;

    fn command() -> FfmpegCommand {
        FfmpegCommand::compile(
            &rskit_storage::FileSource::from_bytes(bytes::Bytes::from_static(b"media")),
            &[],
            None,
            &FfmpegConfig::default(),
            &Registry::default(),
        )
        .unwrap()
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runner_invokes_progress_callback_for_stderr_progress_lines() {
        let script = write_script("echo 'time=00:00:01.00 speed=2.0x' >&2\nexit 0");
        let config = FfmpegConfig::default().with_ffmpeg_path(script.path());
        let output = rskit_storage::TempFile::with_extension("mp4").unwrap();
        let callbacks = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&callbacks);

        command()
            .run_with_cancel(
                &config,
                Some(Box::new(move |_| {
                    seen.fetch_add(1, Ordering::SeqCst);
                })),
                output.path(),
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(callbacks.load(Ordering::SeqCst), 1);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runner_classifies_failed_exit_and_truncates_stderr_window() {
        let script = write_script(
            "echo 'first line' >&2\necho 'invalid data found when processing input' >&2\nexit 1",
        );
        let config = FfmpegConfig::default()
            .with_ffmpeg_path(script.path())
            .with_max_stderr_lines(1);
        let output = rskit_storage::TempFile::with_extension("mp4").unwrap();

        let error = command()
            .run_with_cancel(&config, None, output.path(), CancellationToken::new())
            .await
            .unwrap_err();

        assert_eq!(error.kind, crate::error::FfmpegErrorKind::InvalidInput);
        assert_eq!(error.exit_code, Some(1));
        assert!(error.stderr.contains("invalid data found"));
        assert!(!error.stderr.contains("first line"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runner_reports_cancelled_and_timed_out_processes() {
        let output = rskit_storage::TempFile::with_extension("mp4").unwrap();

        let cancel_script = write_script("while true; do :; done");
        let cancel_config = FfmpegConfig::default().with_ffmpeg_path(cancel_script.path());
        let cancel = CancellationToken::new();
        cancel.cancel();
        let cancelled = command()
            .run_with_cancel(&cancel_config, None, output.path(), cancel)
            .await
            .unwrap_err();
        assert_eq!(cancelled.kind, crate::error::FfmpegErrorKind::Cancelled);

        let timeout_script = write_script("echo 'still running' >&2\nwhile true; do :; done");
        let timeout_config = FfmpegConfig::default()
            .with_ffmpeg_path(timeout_script.path())
            .with_timeout(std::time::Duration::from_millis(1));
        let timed_out = command()
            .run_with_cancel(
                &timeout_config,
                None,
                output.path(),
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert_eq!(timed_out.kind, crate::error::FfmpegErrorKind::Timeout);
    }

    #[test]
    fn stderr_ring_buffer_keeps_last_lines() {
        let lines = Mutex::new(VecDeque::with_capacity(2));

        push_stderr_line(&lines, 2, "one");
        push_stderr_line(&lines, 2, "two");
        push_stderr_line(&lines, 2, "three");

        assert_eq!(
            lines.lock().iter().cloned().collect::<Vec<_>>(),
            vec!["two".to_string(), "three".to_string()]
        );
    }
}
