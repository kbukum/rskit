//! Native image backend registration.

use std::sync::Arc;

use crate::config::Config;
use crate::probe::ImageProbe;
use crate::processor::ImageProcessor;

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
            Ok(Arc::new(ImageProcessor::new(Arc::clone(&processor_config)))
                as Arc<dyn rskit_media::MediaExecutor>)
        }),
    )?;
    let probe_config = Arc::clone(&config);
    registry.register_probe(
        "image",
        Arc::new(move || {
            Ok(Arc::new(ImageProbe::new(Arc::clone(&probe_config)))
                as Arc<dyn rskit_media::MediaProbe>)
        }),
    )
}
