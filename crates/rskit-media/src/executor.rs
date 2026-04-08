//! Backend executor trait for media processing.

use rskit_errors::AppResult;
use rskit_file::{FileSink, FileSource};
use tokio_util::sync::CancellationToken;

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

    /// Execute with cancellation support.
    ///
    /// The default implementation ignores the token and delegates to
    /// [`execute`](MediaExecutor::execute). Backends should override this
    /// to honour cancellation (e.g. kill a subprocess).
    async fn execute_cancellable(
        &self,
        source: &FileSource,
        ops: &[MediaOp],
        sink: Option<&FileSink>,
        on_progress: Option<Box<dyn Fn(Progress) + Send + Sync>>,
        _cancel: CancellationToken,
    ) -> AppResult<FileSource> {
        match on_progress {
            Some(cb) => self.execute_with_progress(source, ops, sink, cb).await,
            None => self.execute(source, ops, sink).await,
        }
    }

    /// Check if this executor supports a given operation.
    fn supports(&self, op: &MediaOp) -> bool;

    /// Dry run: return the command(s) that would be executed.
    fn preview(&self, source: &FileSource, ops: &[MediaOp]) -> AppResult<Vec<String>>;
}
