//! Local store configuration.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

static NEXT_DEFAULT_ROOT_ID: AtomicU64 = AtomicU64::new(0);

/// Configuration for the local file store.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LocalStoreConfig {
    /// Root directory that all stored keys and generated parent directories must stay under.
    pub root_dir: PathBuf,
    /// Whether to auto-create the root directory if it doesn't exist.
    pub auto_create: bool,
}

impl Default for LocalStoreConfig {
    fn default() -> Self {
        Self {
            root_dir: default_local_root_dir(),
            auto_create: true,
        }
    }
}

fn default_local_root_dir() -> PathBuf {
    let sequence = NEXT_DEFAULT_ROOT_ID.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    std::env::temp_dir().join(format!(
        "rskit-storage-{}-{nanos}-{sequence}",
        std::process::id()
    ))
}
