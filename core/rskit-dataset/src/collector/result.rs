//! Result of a collection run.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::manifest::SourceStats;
use crate::target::PublishResult;

/// Result of a collection run.
#[derive(Debug, Clone, Default)]
pub struct CollectorResult {
    /// Total items emitted or reused from cache.
    pub total_items: usize,
    /// Count of real-labeled items.
    pub real_count: usize,
    /// Count of AI-generated-labeled items.
    pub ai_count: usize,
    /// Per-source collection statistics.
    pub source_stats: HashMap<String, SourceStats>,
    /// Source names skipped because a completed cache entry was available.
    pub cached_sources: Vec<String>,
    /// Results returned by publish targets.
    pub publish_results: Vec<PublishResult>,
    /// Wall-clock duration in seconds.
    pub duration_seconds: f64,
    /// Output directory used by the collector.
    pub output_dir: PathBuf,
}
