//! FFmpeg executor implementation.

use std::sync::Arc;

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_file::{FileSink, FileSource, TempFile};
use rskit_media::{executor::MediaExecutor, ops::MediaOp, pipeline::Progress, registry::Registry};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::{
    command::FfmpegCommand,
    config::FfmpegConfig,
    hw_accel::HwAccel,
};

/// FFmpeg-based media executor with concurrency control and hw accel fallback.
pub struct FfmpegExecutor {
    config: FfmpegConfig,
    registry: Registry,
    semaphore: Arc<Semaphore>,
}

impl FfmpegExecutor {
    /// Create a new executor with the given configuration and registry.
    pub fn new(config: FfmpegConfig, registry: Registry) -> Self {
        let max = config.effective_max_concurrent();
        tracing::info!(max_concurrent = max, "FfmpegExecutor initialized");
        Self {
            semaphore: Arc::new(Semaphore::new(max)),
            config,
            registry,
        }
    }

    /// Check that ffmpeg is available and return its version.
    pub async fn check_available(&self) -> AppResult<String> {
        let output = tokio::process::Command::new(self.config.ffmpeg_bin())
            .arg("-version")
            .output()
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::ServiceUnavailable,
                    format!("ffmpeg not found: {e}"),
                )
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let version = stdout.lines().next().unwrap_or("unknown").to_string();
        Ok(version)
    }

    fn determine_output_extension(&self, ops: &[MediaOp]) -> String {
        for op in ops.iter().rev() {
            if let MediaOp::Transcode(config) = op {
                if let Some(info) = self.registry.format_info(&config.format) {
                    return info.extension.clone();
                }
            }
        }
        "mkv".to_string()
    }

    /// Run an FFmpeg command with concurrency control and optional hw accel fallback.
    async fn run_with_retry(
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

        // Wrap in Arc so the callback survives hw accel fallback retry
        let on_progress: Option<Arc<dyn Fn(Progress) + Send + Sync>> =
            on_progress.map(Arc::from);

        // First attempt with configured hw_accel
        let cmd = FfmpegCommand::compile(source, ops, sink, &self.config, &self.registry)?;
        let progress_cb = on_progress.as_ref().map(|cb| {
            let cb = Arc::clone(cb);
            Box::new(move |p: Progress| cb(p)) as Box<dyn Fn(Progress) + Send + Sync>
        });
        let result = cmd.run(&self.config, progress_cb, &output_file).await;

        match result {
            Ok(()) => Ok(output_file),
            Err(ffmpeg_err) => {
                // Direct access to classified error kind (no information loss)
                let should_fallback = self.config.hw_accel_fallback
                    && self.config.hw_accel.as_ref().is_some_and(|hw| hw.is_hardware())
                    && ffmpeg_err.kind.is_retryable();

                if should_fallback && ffmpeg_err.kind.should_fallback_hw_accel() {
                    tracing::warn!(
                        error = %ffmpeg_err.message,
                        kind = ?ffmpeg_err.kind,
                        exit_code = ?ffmpeg_err.exit_code,
                        "FFmpeg hw accel failed, retrying with software decode"
                    );

                    // Build a config with hw accel disabled
                    let mut fallback_config = self.config.clone();
                    fallback_config.hw_accel = Some(HwAccel::None);

                    // Clean up any partial output from the failed attempt
                    let _ = tokio::fs::remove_file(&output_file).await;

                    let cmd_fallback = FfmpegCommand::compile(
                        source,
                        ops,
                        sink,
                        &fallback_config,
                        &self.registry,
                    )?;

                    // Re-wrap the callback for the retry attempt
                    let progress_cb = on_progress.as_ref().map(|cb| {
                        let cb = Arc::clone(cb);
                        Box::new(move |p: Progress| cb(p))
                            as Box<dyn Fn(Progress) + Send + Sync>
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
}

#[async_trait::async_trait]
impl MediaExecutor for FfmpegExecutor {
    async fn execute(
        &self,
        source: &FileSource,
        ops: &[MediaOp],
        sink: Option<&FileSink>,
    ) -> AppResult<FileSource> {
        let output_file = self.run_with_retry(source, ops, sink, None).await?;

        match sink {
            Some(FileSink::Path(p)) => Ok(FileSource::Path(p.clone())),
            Some(FileSink::Memory) => {
                let data = tokio::fs::read(&output_file).await.map_err(|e| {
                    AppError::new(ErrorCode::Internal, format!("read output failed: {e}"))
                })?;
                Ok(FileSource::Bytes(bytes::Bytes::from(data)))
            }
            _ => Ok(FileSource::Path(output_file)),
        }
    }

    async fn execute_with_progress(
        &self,
        source: &FileSource,
        ops: &[MediaOp],
        sink: Option<&FileSink>,
        on_progress: Box<dyn Fn(Progress) + Send + Sync>,
    ) -> AppResult<FileSource> {
        let output_file = self
            .run_with_retry(source, ops, sink, Some(on_progress))
            .await?;

        match sink {
            Some(FileSink::Path(p)) => Ok(FileSource::Path(p.clone())),
            Some(FileSink::Memory) => {
                let data = tokio::fs::read(&output_file).await.map_err(|e| {
                    AppError::new(ErrorCode::Internal, format!("read output failed: {e}"))
                })?;
                Ok(FileSource::Bytes(bytes::Bytes::from(data)))
            }
            _ => Ok(FileSource::Path(output_file)),
        }
    }

    fn supports(&self, op: &MediaOp) -> bool {
        // All ops are now implemented
        let _ = op;
        true
    }

    fn preview(&self, source: &FileSource, ops: &[MediaOp]) -> AppResult<Vec<String>> {
        let cmd = FfmpegCommand::compile(source, ops, None, &self.config, &self.registry)?;
        let mut args = vec![self.config.ffmpeg_bin().to_string_lossy().to_string()];
        args.extend(cmd.to_args());
        args.push("<output>".into());
        Ok(vec![args.join(" ")])
    }

    async fn execute_cancellable(
        &self,
        source: &FileSource,
        ops: &[MediaOp],
        sink: Option<&FileSink>,
        on_progress: Option<Box<dyn Fn(Progress) + Send + Sync>>,
        cancel: CancellationToken,
    ) -> AppResult<FileSource> {
        let run_fut = self.run_with_retry(source, ops, sink, on_progress);

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                tracing::info!("media pipeline cancelled by token");
                Err(AppError::new(ErrorCode::Cancelled, "media pipeline cancelled"))
            }
            result = run_fut => {
                let output_file = result?;
                match sink {
                    Some(FileSink::Path(p)) => Ok(FileSource::Path(p.clone())),
                    Some(FileSink::Memory) => {
                        let data = tokio::fs::read(&output_file).await.map_err(|e| {
                            AppError::new(ErrorCode::Internal, format!("read output failed: {e}"))
                        })?;
                        Ok(FileSource::Bytes(bytes::Bytes::from(data)))
                    }
                    _ => Ok(FileSource::Path(output_file)),
                }
            }
        }
    }
}
