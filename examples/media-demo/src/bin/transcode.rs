//! Transcode a video using a pipeline.
//!
//! Usage:
//!   cargo run --bin transcode -- input.mp4 output.mp4
//!
//! File paths are confined to the current working directory.

use rskit::media::{
    Registry, filter::filters, ops::ResizeMode, pipeline::MediaPipeline, presets,
    spatial::Resolution, time::TimeRange,
};
use rskit::media_ffmpeg::register as register_ffmpeg;
use rskit::storage::{FileSink, FileSource};

#[tokio::main]
async fn main() -> rskit::AppResult<()> {
    let args: Vec<String> = std::env::args().collect();
    let input = args.get(1).map(String::as_str).unwrap_or("input.mp4");
    let output = args.get(2).map(String::as_str).unwrap_or("output.mp4");

    let source = FileSource::from_path(input);
    let sink = FileSink::Path(output.into());
    let mut registry = Registry::default();
    register_ffmpeg(&mut registry, media_demo::ffmpeg_config()?)?;
    let executor = registry.executor("ffmpeg")?;

    let result = MediaPipeline::from(&source)
        .extract(TimeRange::from_seconds(0.0, 30.0))
        .resize(Resolution::p720(), ResizeMode::Fit)
        .filter(filters::denoise(3))
        .volume(0.9)
        .transcode(presets::mp4_h264())
        .output_to(sink)
        .execute(executor.as_ref())
        .await?;

    println!("Transcode complete: {:?}", result);
    Ok(())
}
