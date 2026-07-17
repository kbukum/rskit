//! Progress callback protocol for reporting collection progress.

use crate::manifest::SourceStats;
use crate::target::PublishResult;

/// Callback protocol for reporting collection progress.
///
/// Called **only from the main event loop** — never from worker tasks.
/// This means implementations do NOT need interior mutability or `Sync`.
///
/// All methods have default no-op implementations, so you only override
/// what you need.
pub trait ProgressCallback: Send {
    /// Called when a source starts streaming.
    fn on_source_start(&self, _index: usize, _name: &str, _max_items: Option<usize>) {}
    /// Called after a source emits or resumes item progress.
    fn on_source_progress(&self, _index: usize, _count: usize) {}
    /// Called when a source finishes successfully or partially.
    fn on_source_done(&self, _index: usize, _name: &str, _stats: &SourceStats) {}
    /// Called when a source is skipped from manifest cache.
    fn on_source_cached(&self, _index: usize, _name: &str, _stats: &SourceStats) {}
    /// Called when source processing fails.
    fn on_source_error(&self, _index: usize, _name: &str, _error: &str) {}
    /// Called before publishing to a target.
    fn on_publish_start(&self, _target: &str) {}
    /// Called after a target publishes successfully.
    fn on_publish_done(&self, _target: &str, _result: &PublishResult) {}
    /// Called when target publishing fails.
    fn on_publish_error(&self, _target: &str, _error: &str) {}
}

/// No-op progress callback (all defaults).
pub struct NullProgress;
impl ProgressCallback for NullProgress {}
