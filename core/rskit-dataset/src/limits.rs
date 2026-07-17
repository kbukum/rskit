use serde::{Deserialize, Serialize};

/// Default threshold above which payloads should be represented as files.
pub const DEFAULT_MAX_IN_MEMORY_BYTES: usize = 8 * 1024 * 1024;

/// Runtime limits for dataset streaming and bounded materialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatasetLimits {
    /// Largest payload that may be held in memory by bounded helpers.
    pub max_in_memory_bytes: usize,
    /// Bounded channel capacity for source/collector stream plumbing.
    pub stream_buffer: usize,
}

impl Default for DatasetLimits {
    fn default() -> Self {
        Self {
            max_in_memory_bytes: DEFAULT_MAX_IN_MEMORY_BYTES,
            stream_buffer: 64,
        }
    }
}
