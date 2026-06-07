//! Native image processing backend.
//!
//! Uses the `image` crate for fast image operations without
//! requiring FFmpeg to be installed.

#![warn(missing_docs)]

mod config;
mod io;
mod probe;
mod processor;

use std::sync::Arc;

pub use config::Config;

/// Register configured native image executor and probe factories.
pub fn register(
    registry: &mut rskit_media::Registry,
    config: Config,
) -> rskit_errors::AppResult<()> {
    let config = Arc::new(config);
    let processor_config = Arc::clone(&config);
    registry.register_executor(
        "image",
        Arc::new(move || {
            Ok(Arc::new(processor::ImageProcessor::new(Arc::clone(
                &processor_config,
            ))) as Arc<dyn rskit_media::MediaExecutor>)
        }),
    )?;
    let probe_config = Arc::clone(&config);
    registry.register_probe(
        "image",
        Arc::new(move || {
            Ok(Arc::new(probe::ImageProbe::new(Arc::clone(&probe_config)))
                as Arc<dyn rskit_media::MediaProbe>)
        }),
    )
}
