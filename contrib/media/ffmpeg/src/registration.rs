use std::sync::Arc;

use crate::{config::FfmpegConfig as Config, executor, probe};

/// Register configured FFmpeg media executor and probe factories.
pub fn register(
    registry: &mut rskit_media::Registry,
    config: Config,
) -> rskit_errors::AppResult<()> {
    let executor_config = config.clone();
    let executor_registry = registry.clone();
    registry.register_executor(
        "ffmpeg",
        Arc::new(move || {
            Ok(Arc::new(executor::FfmpegExecutor::new(
                executor_config.clone(),
                executor_registry.clone(),
            )) as Arc<dyn rskit_media::MediaExecutor>)
        }),
    )?;
    registry.register_probe(
        "ffmpeg",
        Arc::new(move || {
            Ok(Arc::new(probe::FfmpegProbe::new(config.clone()))
                as Arc<dyn rskit_media::MediaProbe>)
        }),
    )
}
