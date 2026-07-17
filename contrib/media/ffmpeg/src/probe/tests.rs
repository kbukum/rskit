use std::time::Duration;

use rskit_errors::ErrorCode;
use rskit_media::{probe::MediaProbe, spatial::Resolution, time::Timestamp};
use rskit_storage::FileSource;

use super::FfmpegProbe;
use crate::config::FfmpegConfig;

#[cfg(unix)]
use crate::test_support::write_executable_script as write_script;

#[cfg(unix)]
#[tokio::test]
async fn probe_raw_reports_invalid_json_from_ffprobe() {
    let ffprobe = write_script("printf 'not-json'");
    let input = rskit_storage::TempFile::with_extension("mp4").unwrap();
    std::fs::write(input.path(), b"media").unwrap();
    let probe = FfmpegProbe::new(FfmpegConfig::default().with_ffprobe_path(ffprobe.path()));

    let error = probe
        .probe_raw(&FileSource::from_path(input.path()))
        .await
        .unwrap_err();

    assert_eq!(error.code(), ErrorCode::Internal);
    assert!(error.message().contains("not valid JSON"));
}

#[tokio::test]
async fn media_probe_trait_methods_delegate_to_ffmpeg_helpers() {
    let missing_bin = rskit_storage::TempDir::new()
        .unwrap()
        .path()
        .join("missing-ffmpeg");
    let probe = FfmpegProbe::new(
        FfmpegConfig::default()
            .with_ffmpeg_path(&missing_bin)
            .with_ffprobe_path(&missing_bin),
    );
    let media_probe: &dyn MediaProbe = &probe;
    let source = FileSource::from_bytes(bytes::Bytes::from_static(b"media"));
    let resolution = Resolution::new(16, 16);

    assert!(media_probe.probe(&source).await.is_err());
    assert!(
        media_probe
            .thumbnail(&source, Timestamp::from_seconds(0.0), Some(resolution))
            .await
            .is_err()
    );
    assert!(
        media_probe
            .thumbnails(&source, Duration::from_secs(1), Some(resolution))
            .await
            .is_err()
    );
    assert!(
        media_probe
            .sprite_sheet(&source, Duration::from_secs(1), resolution, 2)
            .await
            .is_err()
    );
    assert!(media_probe.scene_detect(&source, 0.5).await.is_err());
    assert!(media_probe.waveform(&source, resolution).await.is_err());
    assert!(media_probe.keyframes(&source).await.is_err());
    assert!(
        media_probe
            .silence_detect(&source, Duration::from_secs(1), -30.0)
            .await
            .is_err()
    );
    assert!(media_probe.chapters(&source).await.is_err());
}
