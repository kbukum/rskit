//! Backend executor trait for media processing.

use rskit_errors::AppResult;
use rskit_file::{FileSink, FileSource};

use crate::{ops::MediaOp, pipeline::Progress};

/// Backend that can execute a media pipeline.
#[async_trait::async_trait]
pub trait MediaExecutor: Send + Sync {
    /// Execute a sequence of operations on a source.
    async fn execute(
        &self,
        source: &FileSource,
        ops: &[MediaOp],
        sink: Option<&FileSink>,
    ) -> AppResult<FileSource>;

    /// Execute with progress reporting.
    async fn execute_with_progress(
        &self,
        source: &FileSource,
        ops: &[MediaOp],
        sink: Option<&FileSink>,
        on_progress: Box<dyn Fn(Progress) + Send + Sync>,
    ) -> AppResult<FileSource>;

    /// Check if this executor supports a given operation.
    fn supports(&self, op: &MediaOp) -> bool;

    /// Dry run: return the command(s) that would be executed.
    fn preview(&self, source: &FileSource, ops: &[MediaOp]) -> AppResult<Vec<String>>;
}
