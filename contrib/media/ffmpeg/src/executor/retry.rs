//! Hardware-accelerated execution with automatic fallback.
//!
//! Contains the retry loop that first attempts an FFmpeg command with
//! the configured hardware acceleration, then falls back to software
//! decode on retryable failures (e.g., VideoToolbox AV1 issues).

use std::sync::Arc;

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_media::{ops::MediaOp, pipeline::Progress};
use rskit_storage::{FileSink, FileSource, TempFile};

use crate::command::FfmpegCommand;
use crate::config::FfmpegConfig;
use crate::hw_accel::HwAccel;
use crate::process::run_capture_lossy_with_cancel;
use tokio_util::sync::CancellationToken;

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
        self.run_with_retry_cancelable(source, ops, sink, on_progress, CancellationToken::new())
            .await
    }

    /// Run an FFmpeg command with retry and cancellation support.
    pub(crate) async fn run_with_retry_cancelable(
        &self,
        source: &FileSource,
        ops: &[MediaOp],
        sink: Option<&FileSink>,
        on_progress: Option<Box<dyn Fn(Progress) + Send + Sync>>,
        cancel: CancellationToken,
    ) -> AppResult<std::path::PathBuf> {
        let ext = self.determine_output_extension(ops);
        let output = prepare_output_path(&self.config, &ext, sink)?;
        if output.is_user_path {
            crate::paths::create_output_parent(&self.config, &output.path).await?;
        }
        let output_file = output.path;

        let _permit = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                return Err(AppError::new(ErrorCode::Cancelled, "media pipeline cancelled"));
            }
            permit = self.semaphore.acquire() => {
                permit.map_err(|_| AppError::new(ErrorCode::Internal, "semaphore closed"))?
            }
        };

        tracing::debug!(
            available_permits = self.semaphore.available_permits(),
            "acquired FFmpeg semaphore permit"
        );

        // Build source hints by quick-probing when concat-style ops are present
        let hints = self.build_source_hints(source, ops, cancel.clone()).await?;

        // Resolve duration-aware timeout: probe source duration and infer
        // operation kind so the timeout scales with content length.
        let effective_config = self
            .resolve_effective_config(source, ops, cancel.clone())
            .await?;

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
        let result = cmd
            .run_with_cancel(&effective_config, progress_cb, &output_file, cancel.clone())
            .await;

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
                        if let Some(sw_decoder) =
                            Self::find_sw_av1_decoder(&self.config, cancel.clone()).await?
                        {
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
                        .run_with_cancel(&fallback_config, progress_cb, &output_file, cancel)
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
    async fn find_sw_av1_decoder(
        config: &FfmpegConfig,
        cancel: CancellationToken,
    ) -> AppResult<Option<String>> {
        let output = run_capture_lossy_with_cancel(
            config.ffmpeg_bin(),
            ["-hide_banner", "-decoders"],
            config.timeout,
            cancel,
        )
        .await
        .map_err(|error| {
            if error.code() == ErrorCode::Cancelled {
                error
            } else {
                AppError::new(
                    ErrorCode::Internal,
                    format!("failed to query software AV1 decoders: {error}"),
                )
            }
        })?;

        // Prefer libdav1d (fastest), then libaom-av1, then libgav1
        for decoder in &["libdav1d", "libaom-av1", "libgav1"] {
            if output.stdout.lines().any(|line| line.contains(decoder)) {
                return Ok(Some((*decoder).to_string()));
            }
        }

        Ok(None)
    }
}

#[derive(Debug)]
struct PreparedOutputPath {
    path: std::path::PathBuf,
    is_user_path: bool,
}

fn prepare_output_path(
    config: &FfmpegConfig,
    ext: &str,
    sink: Option<&FileSink>,
) -> AppResult<PreparedOutputPath> {
    match sink {
        Some(FileSink::Path(path)) => Ok(PreparedOutputPath {
            path: crate::paths::confine_output_path(config, path)?,
            is_user_path: true,
        }),
        _ => Ok(PreparedOutputPath {
            path: TempFile::with_extension(ext)?.path().to_path_buf(),
            is_user_path: false,
        }),
    }
}

#[cfg(test)]
mod tests {
    use rskit_storage::{FileSink, TempDir};

    use super::*;

    #[test]
    fn prepare_output_path_confines_user_sink_paths() {
        let root = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let config = FfmpegConfig::default().with_path_root(root.path());

        let error = prepare_output_path(
            &config,
            "mkv",
            Some(&FileSink::Path(outside.path().join("out.mkv"))),
        )
        .unwrap_err();

        assert_eq!(error.code(), ErrorCode::InvalidInput);
    }

    #[test]
    fn prepare_output_path_does_not_confine_internal_temp_outputs() {
        let root = TempDir::new().unwrap();
        let config = FfmpegConfig::default().with_path_root(root.path());

        let output = prepare_output_path(&config, "mkv", Some(&FileSink::Memory)).unwrap();

        assert!(!output.is_user_path);
        assert!(!output.path.starts_with(root.path()));
    }

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
