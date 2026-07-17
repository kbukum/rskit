use rskit_errors::AppResult;
use rskit_media::{executor::MediaExecutor, ops::MediaOp, pipeline::Progress};
use rskit_storage::{FileSink, FileSource};
use tokio_util::sync::CancellationToken;

use crate::command::FfmpegCommand;

use super::{FfmpegExecutor, output::resolve_output};

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
