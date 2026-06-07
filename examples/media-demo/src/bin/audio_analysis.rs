//! Analyse a WAV file through the media registry.
//!
//! Usage:
//!   cargo run --bin audio_analysis -- path/to/audio.wav

use std::time::Duration;

use rskit::storage::FileSource;

#[tokio::main]
async fn main() -> rskit::AppResult<()> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "input.wav".into());

    let mut registry = rskit::media::Registry::default();
    rskit::media_audio::register(&mut registry, rskit::media_audio::Config::default())?;
    let probe = registry.probe("audio")?;
    let source = FileSource::from_path(&path);
    let metadata = probe.probe(&source).await?;

    println!("=== WAV Info: {path} ===");
    if let Some(track) = metadata
        .audio_track()
        .and_then(|track| track.audio.as_ref())
    {
        println!("Channels     : {}", track.channels.channel_count());
        println!("Sample rate  : {} Hz", track.sample_rate.0);
        if let Some(bit_depth) = track.bit_depth {
            println!("Bits/sample  : {bit_depth}");
        }
    }
    if let Some(duration) = metadata.duration {
        println!("Duration     : {:.2} s", duration.as_secs_f64());
    }
    if let Some(bitrate) = metadata.bitrate {
        println!("Bitrate      : {bitrate} bps");
    }

    println!("\n=== Loudness ===");
    println!(
        "Peak    : {} dBFS",
        metadata
            .tags
            .get("audio.peak_db")
            .map_or("unknown", String::as_str)
    );
    println!(
        "RMS     : {} dBFS",
        metadata
            .tags
            .get("audio.rms_db")
            .map_or("unknown", String::as_str)
    );
    println!(
        "LUFS    : {}",
        metadata
            .tags
            .get("audio.lufs")
            .map_or("unknown", String::as_str)
    );

    let silences = probe
        .silence_detect(&source, Duration::from_millis(500), -40.0)
        .await?;
    println!("\n=== Silence regions ({}) ===", silences.len());
    for (i, silence) in silences.iter().enumerate() {
        println!(
            "  {i}: {:.2}s - {:.2}s ({:.2}s)",
            silence.start.as_seconds(),
            silence.end.as_seconds(),
            silence.duration.as_secs_f64()
        );
    }

    Ok(())
}
