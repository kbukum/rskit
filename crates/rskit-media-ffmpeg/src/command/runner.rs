//! Process execution for compiled FFmpeg commands.
//!
//! Handles spawning the `ffmpeg` process, stderr streaming for progress
//! and error diagnostics, timeout enforcement, and process-group cleanup.

use std::sync::Arc;

use rskit_media::pipeline::Progress;

use crate::config::FfmpegConfig;
use crate::progress::FfmpegProgressParser;

use super::FfmpegCommand;

#[cfg(unix)]
#[allow(unused_imports)]
use std::os::unix::process::CommandExt;

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
        let mut args = self.to_args();
        args.push(output_path.to_string_lossy().to_string());

        tracing::debug!(cmd = %format!("ffmpeg {}", args.join(" ")), "executing ffmpeg");

        let mut command = tokio::process::Command::new(config.ffmpeg_bin());
        command
            .args(&args)
            .stderr(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stdin(std::process::Stdio::null());

        // Process group isolation on Unix — allows clean SIGTERM of all child processes
        #[cfg(unix)]
        unsafe {
            command.pre_exec(|| {
                // Create new process group so we can kill the entire group
                libc::setpgid(0, 0);
                Ok(())
            });
        }

        let mut child = command.spawn().map_err(|e| crate::error::FfmpegError {
            kind: crate::error::FfmpegErrorKind::SpawnFailed,
            exit_code: None,
            stderr: String::new(),
            message: format!("failed to spawn ffmpeg: {e}"),
        })?;

        let child_pid = child.id();

        // Set up stderr reader for both progress parsing and error capture
        // SAFETY: `.stderr(Stdio::piped())` is called above; `take()` on a piped
        // child stderr is always Some.
        let stderr = child
            .stderr
            .take()
            .expect("stderr was piped in command setup");
        let reader = tokio::io::BufReader::new(stderr);
        use tokio::io::AsyncBufReadExt;
        let mut lines = reader.lines();

        // Channel for collecting stderr lines (for error diagnostics)
        let (stderr_tx, mut stderr_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        // Channel for progress updates
        let progress_callback = on_progress.map(Arc::new);

        let stderr_task = tokio::spawn({
            let progress_callback = progress_callback.clone();
            let parser = FfmpegProgressParser::new(None);
            async move {
                while let Ok(Some(line)) = lines.next_line().await {
                    // Try parsing progress
                    if let Some(ref cb) = progress_callback
                        && let Some(progress) = parser.parse_line(&line)
                    {
                        cb(progress);
                    }
                    // Always collect stderr for error diagnostics
                    let _ = stderr_tx.send(line);
                }
            }
        });

        // Wait for child with optional timeout
        let wait_result = if let Some(timeout_dur) = config.timeout {
            match tokio::time::timeout(timeout_dur, child.wait()).await {
                Ok(result) => result.map_err(|e| crate::error::FfmpegError {
                    kind: crate::error::FfmpegErrorKind::Unknown,
                    exit_code: None,
                    stderr: String::new(),
                    message: format!("ffmpeg process error: {e}"),
                }),
                Err(_) => {
                    // Timeout — kill the process
                    tracing::warn!("FFmpeg process timed out after {:?}, killing", timeout_dur);
                    Self::kill_process(&mut child, child_pid);
                    return Err(crate::error::FfmpegError {
                        kind: crate::error::FfmpegErrorKind::Timeout,
                        exit_code: None,
                        stderr: String::new(),
                        message: format!("ffmpeg timed out after {timeout_dur:?}"),
                    });
                }
            }
        } else {
            child.wait().await.map_err(|e| crate::error::FfmpegError {
                kind: crate::error::FfmpegErrorKind::Unknown,
                exit_code: None,
                stderr: String::new(),
                message: format!("ffmpeg process error: {e}"),
            })
        };

        // Wait for stderr reader to finish
        let _ = stderr_task.await;

        // Collect all stderr lines
        let mut stderr_lines = Vec::new();
        while let Ok(line) = stderr_rx.try_recv() {
            stderr_lines.push(line);
        }
        let stderr_output = stderr_lines.join("\n");

        let status = wait_result?;

        if !status.success() {
            let exit_code = status.code();
            let truncated_stderr =
                crate::error::truncate_stderr(&stderr_output, config.max_stderr_lines);
            let kind = crate::error::classify_error(exit_code, &stderr_output);

            let message = format!(
                "ffmpeg exited with status: {} (classified: {:?})",
                status, kind
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

    /// Kill an FFmpeg child process and its process group.
    fn kill_process(child: &mut tokio::process::Child, _pid: Option<u32>) {
        // Try graceful SIGTERM first on Unix
        #[cfg(unix)]
        if let Some(pid) = _pid {
            unsafe {
                // Send SIGTERM to the process group
                libc::kill(-(pid as i32), libc::SIGTERM);
            }
        }

        // Then force kill via tokio
        let _ = child.start_kill();
    }
}
