use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub(crate) const DEFAULT_MAX_ENTRY_BYTES: u64 = 16 * 1024 * 1024;

/// Filesystem cache configuration.
///
/// The root path is supplied when registering the adapter because it is a
/// deployment concern owned by the composition boundary, not common cache
/// selection configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileCacheConfig {
    /// Root directory used to store cache entries.
    pub root: PathBuf,
    /// Optional prefix prepended to every key.
    pub key_prefix: Option<String>,
    /// Maximum serialized cache-entry size accepted by reads and writes.
    #[serde(default = "default_max_entry_bytes")]
    pub max_entry_bytes: u64,
}

impl FileCacheConfig {
    /// Create filesystem cache configuration rooted at `root`.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            key_prefix: None,
            max_entry_bytes: DEFAULT_MAX_ENTRY_BYTES,
        }
    }
}

const fn default_max_entry_bytes() -> u64 {
    DEFAULT_MAX_ENTRY_BYTES
}
