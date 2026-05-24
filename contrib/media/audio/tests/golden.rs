//! Public-surface tests for rskit-media-audio using real WAV fixtures.

use std::path::PathBuf;
use std::time::Duration;

use rskit_media::{MediaType, Registry};
use rskit_storage::FileSource;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("contrib/media")
        .parent()
        .expect("contrib")
        .parent()
        .expect("workspace")
        .join("tests/fixtures")
}

async fn audio_probe() -> rskit_errors::AppResult<std::sync::Arc<dyn rskit_media::MediaProbe>> {
    let mut registry = Registry::default();
    rskit_media_audio::register(&mut registry, rskit_media_audio::Config::default())?;
    registry.probe("audio")
}

#[tokio::test]
async fn golden_wav_reader_ai_generated() -> rskit_errors::AppResult<()> {
    let probe = audio_probe().await?;
    let metadata = probe
        .probe(&FileSource::from_path(
            fixtures_dir().join("audio/ai-generated.wav"),
        ))
        .await?;

    assert_eq!(metadata.media_type, MediaType::Audio);
    assert!(metadata.has_audio());
    assert!(metadata.sample_rate().is_some_and(|rate| rate.0 > 0));
    assert!(
        metadata
            .duration
            .is_some_and(|duration| duration.as_secs_f64() > 0.0)
    );
    assert!(metadata.tags.contains_key("audio.peak_db"));
    assert!(metadata.tags.contains_key("audio.waveform_peak"));
    Ok(())
}

#[tokio::test]
async fn golden_wav_reader_real_voice() -> rskit_errors::AppResult<()> {
    let probe = audio_probe().await?;
    let metadata = probe
        .probe(&FileSource::from_path(
            fixtures_dir().join("audio/real-voice.wav"),
        ))
        .await?;

    assert_eq!(metadata.media_type, MediaType::Audio);
    assert!(metadata.has_audio());
    assert!(metadata.sample_rate().is_some_and(|rate| rate.0 > 0));
    assert!(
        metadata
            .duration
            .is_some_and(|duration| duration.as_secs_f64() > 0.0)
    );
    assert!(metadata.tags.contains_key("audio.rms_db"));
    assert!(metadata.tags.contains_key("audio.waveform_rms"));
    Ok(())
}

#[tokio::test]
async fn golden_silence_detection_real_voice() -> rskit_errors::AppResult<()> {
    let probe = audio_probe().await?;
    let regions = probe
        .silence_detect(
            &FileSource::from_path(fixtures_dir().join("audio/real-voice.wav")),
            Duration::from_millis(50),
            -40.0,
        )
        .await?;

    assert!(regions.iter().all(|region| region.end >= region.start));
    assert!(regions.iter().all(|region| !region.duration.is_zero()));
    Ok(())
}
