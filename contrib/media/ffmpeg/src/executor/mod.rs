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
use retry::PreparedOutputPath;

/// FFmpeg-based media executor with concurrency control and hw accel fallback.
pub(crate) struct FfmpegExecutor {
    config: FfmpegConfig,
    registry: Registry,
    semaphore: Arc<Semaphore>,
}

impl FfmpegExecutor {
    /// Create a new executor with the given configuration and registry.
    pub(crate) fn new(config: FfmpegConfig, registry: Registry) -> Self {
        let max = config.effective_max_concurrent();
        tracing::debug!(max_concurrent = max, "FfmpegExecutor initialized");
        Self {
            semaphore: Arc::new(Semaphore::new(max)),
            config,
            registry,
        }
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
    output: PreparedOutputPath,
    sink: Option<&FileSink>,
) -> AppResult<FileSource> {
    match sink {
        Some(FileSink::Path(_)) => Ok(FileSource::Path(output.path)),
        Some(FileSink::Memory) => {
            let data = tokio::fs::read(&output.path).await.map_err(|e| {
                AppError::new(ErrorCode::Internal, format!("read output failed: {e}"))
            })?;
            Ok(FileSource::Bytes(bytes::Bytes::from(data)))
        }
        _ => match output.temp {
            Some(temp) => Ok(temp.into_source()),
            None => Ok(FileSource::Path(output.path)),
        },
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
        !op.requires_external_tool()
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
        let output_file = self
            .run_with_retry_cancelable(source, ops, sink, on_progress, cancel)
            .await?;
        resolve_output(output_file, sink).await
    }
}

#[cfg(test)]
mod tests {
    use rskit_media::{
        executor::MediaExecutor, format::Format, ops::MediaOp, output::OutputConfig,
        registry::Registry,
    };
    use rskit_storage::{FileSink, FileSource, TempFile};
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::config::FfmpegConfig;

    #[test]
    fn test_config_input_video_decoder_default_is_none() {
        let config = FfmpegConfig::default();
        assert!(config.input_video_decoder.is_none());
    }

    #[test]
    fn output_extension_uses_last_transcode_format_or_default() {
        let executor = FfmpegExecutor::new(FfmpegConfig::default(), Registry::default());
        let ops = [
            MediaOp::Transcode(OutputConfig::new(Format::new("mp4"))),
            MediaOp::Transcode(OutputConfig::new(Format::new("webm"))),
        ];

        assert_eq!(executor.determine_output_extension(&ops), "webm");
        assert_eq!(executor.determine_output_extension(&[]), "mkv");
    }

    #[tokio::test]
    async fn resolve_output_maps_path_memory_and_temp_outputs() {
        let path_output = PreparedOutputPath {
            path: std::path::PathBuf::from("out.mkv"),
            is_user_path: true,
            temp: None,
        };
        let sink = FileSink::Path("out.mkv".into());
        assert!(matches!(
            resolve_output(path_output, Some(&sink)).await.unwrap(),
            FileSource::Path(_)
        ));

        let memory_file = TempFile::with_extension("mkv").unwrap();
        std::fs::write(memory_file.path(), b"media").unwrap();
        let memory_output = PreparedOutputPath {
            path: memory_file.path().to_path_buf(),
            is_user_path: false,
            temp: None,
        };
        assert!(matches!(
            resolve_output(memory_output, Some(&FileSink::Memory))
                .await
                .unwrap(),
            FileSource::Bytes(_)
        ));

        let temp = TempFile::with_extension("mkv").unwrap();
        let temp_output = PreparedOutputPath {
            path: temp.path().to_path_buf(),
            is_user_path: false,
            temp: Some(temp),
        };
        assert!(matches!(
            resolve_output(temp_output, None).await.unwrap(),
            FileSource::Temp(_)
        ));
    }

    #[tokio::test]
    async fn cancellable_execution_returns_cancelled_before_spawning_ffmpeg() {
        let executor = FfmpegExecutor::new(FfmpegConfig::default(), Registry::default());
        let cancel = CancellationToken::new();
        cancel.cancel();

        let error = executor
            .execute_cancellable(
                &FileSource::from_bytes(bytes::Bytes::from_static(b"media")),
                &[],
                None,
                None,
                cancel,
            )
            .await
            .unwrap_err();

        assert_eq!(error.code(), ErrorCode::Cancelled);
    }
}
