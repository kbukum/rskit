//! Probe a media file and print its metadata.
//!
//! Usage:
//!   cargo run --bin probe -- path/to/video.mp4

use rskit_storage::FileSource;
use rskit_media::probe::MediaProbe;
use rskit_media::types::TrackKind;
use rskit_media_ffmpeg::{FfmpegConfig, FfmpegProbe};

#[tokio::main]
async fn main() -> rskit_errors::AppResult<()> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "input.mp4".into());

    let source = FileSource::from_path(&path);
    let probe = FfmpegProbe::new(FfmpegConfig::default());
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
