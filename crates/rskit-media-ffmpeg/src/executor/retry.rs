//! Hardware-accelerated execution with automatic fallback.
//!
//! Contains the retry loop that first attempts an FFmpeg command with
//! the configured hardware acceleration, then falls back to software
//! decode on retryable failures (e.g., VideoToolbox AV1 issues).

use std::sync::Arc;

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_storage::{FileSink, FileSource, TempFile};
use rskit_media::{ops::MediaOp, pipeline::Progress};

use crate::command::FfmpegCommand;
use crate::config::FfmpegConfig;
use crate::hw_accel::HwAccel;

use super::FfmpegExecutor;

impl FfmpegExecutor {
    /// Run an FFmpeg command with concurrency control and optional hw accel fallback.
    pub(crate) async fn run_with_retry(
        &self,
        source: &FileSource,
        ops: &[MediaOp],
        sink: Option<&FileSink>,
        on_progress: Option<Box<dyn Fn(Progress) + Send + Sync>>,
    ) -> AppResult<std::path::PathBuf> {
        let ext = self.determine_output_extension(ops);
        let output_file = match sink {
            Some(FileSink::Path(p)) => {
                if let Some(parent) = p.parent() {
                    tokio::fs::create_dir_all(parent).await.map_err(|e| {
                        AppError::new(ErrorCode::Internal, format!("create dir failed: {e}"))
                    })?;
                }
                p.clone()
            }
            _ => TempFile::with_extension(&ext)?.path().to_path_buf(),
        };

        // Acquire semaphore permit (blocks if max concurrent reached)
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|_| AppError::new(ErrorCode::Internal, "semaphore closed"))?;

        tracing::debug!(
            available_permits = self.semaphore.available_permits(),
            "acquired FFmpeg semaphore permit"
        );

        // Build source hints by quick-probing when concat-style ops are present
        let hints = self.build_source_hints(source, ops).await;

        // Resolve duration-aware timeout: probe source duration and infer
        // operation kind so the timeout scales with content length.
        let effective_config = self.resolve_effective_config(source, ops).await;

        // Wrap in Arc so the callback survives hw accel fallback retry
        let on_progress: Option<Arc<dyn Fn(Progress) + Send + Sync>> = on_progress.map(Arc::from);

        // First attempt with configured hw_accel
        let cmd = FfmpegCommand::compile_with_hints(
            source,
            ops,
            sink,
            &effective_config,
            &self.registry,
            &hints,
        )?;
        let progress_cb = on_progress.as_ref().map(|cb| {
            let cb = Arc::clone(cb);
            Box::new(move |p: Progress| cb(p)) as Box<dyn Fn(Progress) + Send + Sync>
        });
        let result = cmd.run(&effective_config, progress_cb, &output_file).await;

        match result {
            Ok(()) => Ok(output_file),
            Err(ffmpeg_err) => {
                // Direct access to classified error kind (no information loss)
                let should_fallback = self.config.hw_accel_fallback
                    && self
                        .config
                        .hw_accel
                        .as_ref()
                        .is_some_and(|hw| hw.is_hardware())
                    && ffmpeg_err.kind.is_retryable();

                if should_fallback && ffmpeg_err.kind.should_fallback_hw_accel() {
                    tracing::warn!(
                        error = %ffmpeg_err.message,
                        kind = ?ffmpeg_err.kind,
                        exit_code = ?ffmpeg_err.exit_code,
                        "FFmpeg hw accel failed, retrying with software decode"
                    );

                    // Build a config with hw accel disabled
                    let mut fallback_config = effective_config.clone();
                    fallback_config.hw_accel = Some(HwAccel::None);

                    // If stderr indicates AV1 decode failure, try to find a software
                    // AV1 decoder (libdav1d preferred, libaom-av1 as alternative).
                    if Self::is_av1_decode_failure(&ffmpeg_err.stderr) {
                        if let Some(sw_decoder) = Self::find_sw_av1_decoder(&self.config).await {
                            tracing::info!(
                                decoder = %sw_decoder,
                                "Using software AV1 decoder for fallback"
                            );
                            fallback_config.input_video_decoder = Some(sw_decoder);
                        } else {
                            tracing::error!(
                                "No software AV1 decoder available (libdav1d, libaom-av1). \
                                 Install FFmpeg with libdav1d support or download H.264 content."
                            );
                        }
                    }

                    // Clean up any partial output from the failed attempt
                    let _ = tokio::fs::remove_file(&output_file).await;

                    let cmd_fallback = FfmpegCommand::compile_with_hints(
                        source,
                        ops,
                        sink,
                        &fallback_config,
                        &self.registry,
                        &hints,
                    )?;

                    // Re-wrap the callback for the retry attempt
                    let progress_cb = on_progress.as_ref().map(|cb| {
                        let cb = Arc::clone(cb);
                        Box::new(move |p: Progress| cb(p)) as Box<dyn Fn(Progress) + Send + Sync>
                    });

                    cmd_fallback
                        .run(&fallback_config, progress_cb, &output_file)
                        .await
                        .map_err(|e| e.into_app_error())?;
                    Ok(output_file)
                } else {
                    Err(ffmpeg_err.into_app_error())
                }
            }
        }
    }

    /// Check if FFmpeg stderr indicates an AV1-specific decode failure.
    fn is_av1_decode_failure(stderr: &str) -> bool {
        let lower = stderr.to_lowercase();
        // macOS native av1 decoder fails when VideoToolbox doesn't support AV1 hw decode
        (lower.contains("av1") || lower.contains("av01"))
            && (lower.contains("hardware accelerated")
                || lower.contains("failed to get pixel format")
                || lower.contains("function not implemented")
                || lower.contains("decode error rate"))
    }

    /// Query FFmpeg for available software AV1 decoders.
    /// Returns the best available one, or None if none are compiled in.
    async fn find_sw_av1_decoder(config: &FfmpegConfig) -> Option<String> {
        let output = tokio::process::Command::new(config.ffmpeg_bin())
            .args(["-hide_banner", "-decoders"])
            .output()
            .await
            .ok()?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Prefer libdav1d (fastest), then libaom-av1, then libgav1
        for decoder in &["libdav1d", "libaom-av1", "libgav1"] {
            if stdout.lines().any(|line| line.contains(decoder)) {
                return Some((*decoder).to_string());
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_av1_decode_failure_macos_videotoolbox() {
        let stderr = "Your platform doesn't support hardware accelerated AV1 decoding. \
                       Please use another codec or try software decode.";
        assert!(FfmpegExecutor::is_av1_decode_failure(stderr));
    }

    #[test]
    fn test_av1_decode_failure_pixel_format() {
        let stderr = "av1_videotoolbox: failed to get pixel format from hardware output";
        assert!(FfmpegExecutor::is_av1_decode_failure(stderr));
    }

    #[test]
    fn test_av1_decode_failure_av01_codec() {
        let stderr = "Stream #0:0: Video: av01, hardware accelerated decode not available";
        assert!(FfmpegExecutor::is_av1_decode_failure(stderr));
    }

    #[test]
    fn test_not_av1_decode_failure_h264() {
        let stderr = "h264: hardware accelerated decode failed";
        assert!(!FfmpegExecutor::is_av1_decode_failure(stderr));
    }

    #[test]
    fn test_not_av1_decode_failure_unrelated() {
        let stderr = "Error muxing: permission denied";
        assert!(!FfmpegExecutor::is_av1_decode_failure(stderr));
    }
}
