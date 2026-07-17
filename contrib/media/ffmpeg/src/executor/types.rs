use std::sync::Arc;

use rskit_media::{ops::MediaOp, registry::Registry};
use tokio::sync::Semaphore;

use crate::config::FfmpegConfig;

/// FFmpeg-based media executor with concurrency control and hw accel fallback.
pub(crate) struct FfmpegExecutor {
    pub(crate) config: FfmpegConfig,
    pub(crate) registry: Registry,
    pub(crate) semaphore: Arc<Semaphore>,
}

impl FfmpegExecutor {
    /// Create a new executor with the given configuration and registry.
    pub(crate) fn new(config: FfmpegConfig, registry: Registry) -> Self {
        let max = config.effective_max_concurrent();
        tracing::debug!(max_concurrent = max, "FfmpegExecutor initialized");
        Self {
            semaphore: Arc::new(Semaphore::new(max)),
            config,
            registry,
        }
    }

    pub(crate) fn determine_output_extension(&self, ops: &[MediaOp]) -> String {
        for op in ops.iter().rev() {
            if let MediaOp::Transcode(config) = op
                && let Some(info) = self.registry.format_info(&config.format)
            {
                return info.extension.clone();
            }
        }
        "mkv".to_string()
    }
}
