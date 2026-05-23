//! Source trait — pull data from any origin.

use crate::DataItem;
use rskit_errors::AppResult;
use std::pin::Pin;
use tokio_util::sync::CancellationToken;

/// Boxed dataset stream emitted by a source.
pub type BoxDataStream = Pin<Box<dyn futures::Stream<Item = AppResult<DataItem>> + Send + 'static>>;

/// Protocol for dataset sources.
pub trait Source: Send + Sync {
    /// Stable source identifier used in manifests.
    fn name(&self) -> &str;

    /// Human-readable source label.
    fn display_name(&self) -> &str {
        self.name().rsplit('/').next().unwrap_or(self.name())
    }

    /// Stream items from this source.
    fn stream(self: Box<Self>, cancel: CancellationToken) -> BoxDataStream;

    /// Stable cache key describing this source's configured inputs.
    fn cache_key(&self) -> serde_json::Value;
    /// Optional maximum number of items this source will emit.
    fn max_items(&self) -> Option<usize>;

    /// Configure resume state before streaming.
    fn set_resume_state(&mut self, _offset: usize, _already_fetched: usize) {}
}
