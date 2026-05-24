//! Native image processing backend.
//!
//! Uses the `image` crate for fast image operations without
//! requiring FFmpeg to be installed.

#![warn(missing_docs)]

mod probe;
mod processor;

use std::sync::Arc;

#[doc(hidden)]
pub mod __private {
    pub use crate::probe::ImageProbe;
    pub use crate::processor::ImageProcessor;
}

/// Configuration for the native image media backend.
#[derive(Debug, Clone, Default)]
pub struct Config;

/// Register configured native image executor and probe factories.
pub fn register(
    registry: &mut rskit_media::Registry,
    _config: Config,
) -> rskit_errors::AppResult<()> {
    registry.register_executor(
        "image",
        Arc::new(|| {
            Ok(Arc::new(processor::ImageProcessor::new()) as Arc<dyn rskit_media::MediaExecutor>)
        }),
    )?;
    registry.register_probe(
        "image",
        Arc::new(|| Ok(Arc::new(probe::ImageProbe::new()) as Arc<dyn rskit_media::MediaProbe>)),
    )
}
