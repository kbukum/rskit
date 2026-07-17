//! Collector configuration.

use std::path::PathBuf;

use crate::DatasetLimits;

/// Configuration for the collector.
#[derive(Debug, Clone)]
pub struct CollectorConfig {
    /// Directory where dataset output and manifest files are written.
    pub output_dir: PathBuf,
    /// Maximum number of sources processed concurrently; values below 1 run a single worker.
    pub concurrency: usize,
    /// Per-source timeout in seconds. Non-positive means no timeout.
    pub source_timeout_secs: f64,
    /// Ignore manifest cache and rebuild from sources.
    pub force: bool,
    /// Dataset streaming and materialization limits.
    pub limits: DatasetLimits,
}

impl Default for CollectorConfig {
    fn default() -> Self {
        Self {
            output_dir: PathBuf::from("dataset_build"),
            concurrency: 4,
            source_timeout_secs: 600.0,
            force: false,
            limits: DatasetLimits::default(),
        }
    }
}
