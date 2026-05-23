//! FFmpeg executor — concurrency-controlled media processing with hw accel fallback.

mod resolve;
mod retry;

use std::sync::Arc;

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_media::{executor::MediaExecutor, ops::MediaOp, pipeline::Progress, registry::Registry};
use rskit_storage::{FileSink, FileSource};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::command::FfmpegCommand;
use crate::config::FfmpegConfig;
use crate::process::run_capture_lossy;

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
        tracing::debug!(max_concurrent = max, "FfmpegExecutor initialized");
        Self {
            semaphore: Arc::new(Semaphore::new(max)),
            config,
            registry,
        }
    }

    /// Check that ffmpeg is available and return its version.
    pub async fn check_available(&self) -> AppResult<String> {
        let output = run_capture_lossy(self.config.ffmpeg_bin(), ["-version"], self.config.timeout)
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::ServiceUnavailable,
                    format!("ffmpeg not found: {e}"),
                )
            })?;

        let version = output
            .stdout
            .lines()
            .next()
            .unwrap_or("unknown")
            .to_string();
        Ok(version)
    }

    fn determine_output_extension(&self, ops: &[MediaOp]) -> String {
        for op in ops.iter().rev() {
            if let MediaOp::Transcode(config) = op
                && let Some(info) = self.registry.format_info(&config.format)
            {
                return info.extension.clone();
            }
        }
        "mkv".to_string()
    }
}

/// Convert run_with_retry's output path into the appropriate [`FileSource`]
/// based on the requested sink type.
async fn resolve_output(
    output_file: std::path::PathBuf,
    sink: Option<&FileSink>,
) -> AppResult<FileSource> {
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

#[async_trait::async_trait]
impl MediaExecutor for FfmpegExecutor {
    async fn execute(
        &self,
        source: &FileSource,
        ops: &[MediaOp],
        sink: Option<&FileSink>,
    ) -> AppResult<FileSource> {
        let output_file = self.run_with_retry(source, ops, sink, None).await?;
        resolve_output(output_file, sink).await
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
        resolve_output(output_file, sink).await
    }

    fn supports(&self, op: &MediaOp) -> bool {
        // Upscale and Interpolate require external AI tools, not FFmpeg
        !matches!(op, MediaOp::Upscale(_) | MediaOp::Interpolate(_))
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
                resolve_output(output_file, sink).await
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::config::FfmpegConfig;

    #[test]
    fn test_config_input_video_decoder_default_is_none() {
        let config = FfmpegConfig::default();
        assert!(config.input_video_decoder.is_none());
    }
}
