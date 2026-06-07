//! Extract a thumbnail from a video at a given timestamp.
//!
//! Usage:
//!   cargo run --bin thumbnail -- input.mp4 thumb.jpg 5.0
//!
//! File paths are confined to the current working directory.

use rskit_media::{
    Registry, ops::ResizeMode, pipeline::MediaPipeline, presets, spatial::Resolution,
    time::TimeRange,
};
use rskit_media_ffmpeg::register as register_ffmpeg;
use rskit_storage::{FileSink, FileSource};

#[tokio::main]
async fn main() -> rskit_errors::AppResult<()> {
    let args: Vec<String> = std::env::args().collect();
    let input = args.get(1).map(String::as_str).unwrap_or("input.mp4");
    let output = args.get(2).map(String::as_str).unwrap_or("thumb.jpg");
    let time: f64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(5.0);

    let source = FileSource::from_path(input);
    let sink = FileSink::Path(output.into());
    let mut registry = Registry::default();
    register_ffmpeg(&mut registry, media_demo::ffmpeg_config()?)?;
    let executor = registry.executor("ffmpeg")?;

    let result = MediaPipeline::from(&source)
        .extract(TimeRange::from_seconds(time, time + 0.1))
        .resize(Resolution::new(640, 360), ResizeMode::Fit)
        .transcode(presets::jpeg())
        .output_to(sink)
        .execute(executor.as_ref())
        .await?;

    println!("Thumbnail saved to {output}: {:?}", result);
    Ok(())
}
