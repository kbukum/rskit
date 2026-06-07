//! Probe a media file and print its metadata.
//!
//! Usage:
//!   cargo run --bin probe -- path/to/video.mp4
//!
//! File paths are confined to the current working directory.

use rskit_media::Registry;
use rskit_media::types::TrackKind;
use rskit_media_ffmpeg::register as register_ffmpeg;
use rskit_storage::FileSource;

#[tokio::main]
async fn main() -> rskit_errors::AppResult<()> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "input.mp4".into());

    let source = FileSource::from_path(&path);
    let mut registry = Registry::default();
    register_ffmpeg(&mut registry, media_demo::ffmpeg_config()?)?;
    let probe = registry.probe("ffmpeg")?;
    let info = probe.probe(&source).await?;

    println!("=== Media Info: {path} ===");
    println!("Duration : {:?}", info.duration);
    println!("Format   : {:?}", info.format);
    println!("Tracks   : {}", info.tracks.len());

    for track in &info.tracks {
        println!("  [{:?}] codec={:?}", track.kind, track.codec);
        match track.kind {
            TrackKind::Video => {
                if let Some(v) = &track.video {
                    println!(
                        "    {}×{} @ {:?} fps, bit_depth={:?}",
                        v.resolution.width, v.resolution.height, v.frame_rate, v.bit_depth,
                    );
                }
            }
            TrackKind::Audio => {
                if let Some(a) = &track.audio {
                    println!("    {:?}, {:?}", a.sample_rate, a.channels);
                }
            }
            _ => {}
        }
    }

    Ok(())
}
