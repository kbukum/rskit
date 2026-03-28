//! FFmpeg executor implementation.

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_file::{FileSink, FileSource, TempFile};
use rskit_media::{
    executor::MediaExecutor,
    ops::MediaOp,
    pipeline::Progress,
    registry::Registry,
};

use crate::{command::FfmpegCommand, config::FfmpegConfig};

/// FFmpeg-based media executor.
pub struct FfmpegExecutor {
    config: FfmpegConfig,
    registry: Registry,
}

impl FfmpegExecutor {
    /// Create a new executor with the given configuration and registry.
    pub fn new(config: FfmpegConfig, registry: Registry) -> Self {
        Self { config, registry }
    }

    /// Check that ffmpeg is available and return its version.
    pub async fn check_available(&self) -> AppResult<String> {
        let output = tokio::process::Command::new(self.config.ffmpeg_bin())
            .arg("-version")
            .output()
            .await
            .map_err(|e| {
                AppError::new(ErrorCode::ServiceUnavailable, format!("ffmpeg not found: {e}"))
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let version = stdout.lines().next().unwrap_or("unknown").to_string();
        Ok(version)
    }

    fn determine_output_extension(&self, ops: &[MediaOp]) -> &str {
        for op in ops.iter().rev() {
            if let MediaOp::Transcode(config) = op {
                if let Some(info) = self.registry.format_info(&config.format) {
                    return Box::leak(info.extension.clone().into_boxed_str());
                }
            }
        }
        "mkv" // default container
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
            _ => TempFile::with_extension(ext)?.path().to_path_buf(),
        };

        let cmd = FfmpegCommand::compile(source, ops, sink, &self.config, &self.registry)?;
        cmd.run(&self.config, None, &output_file).await?;

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
        let ext = self.determine_output_extension(ops);
        let output_file = match sink {
            Some(FileSink::Path(p)) => p.clone(),
            _ => TempFile::with_extension(ext)?.path().to_path_buf(),
        };

        let cmd = FfmpegCommand::compile(source, ops, sink, &self.config, &self.registry)?;
        cmd.run(&self.config, Some(on_progress), &output_file).await?;

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
        // FFmpeg supports all operations
        match op {
            MediaOp::BurnSubtitles(_) => true,
            MediaOp::ExtractMany(_) => true,
            _ => true,
        }
    }

    fn preview(&self, source: &FileSource, ops: &[MediaOp]) -> AppResult<Vec<String>> {
        let cmd =
            FfmpegCommand::compile(source, ops, None, &self.config, &self.registry)?;
        let mut args = vec![self.config.ffmpeg_bin().to_string_lossy().to_string()];
        args.extend(cmd.to_args());
        args.push("<output>".into());
        Ok(vec![args.join(" ")])
    }
}
