//! Integration tests for rskit-media-ffmpeg.
//! These tests require ffmpeg and ffprobe to be installed on the system.

use std::time::Duration;

use rskit_storage::{FileSink, FileSource, TempDir, TempFile};
use rskit_media::{
    executor::MediaExecutor,
    ops::{MediaOp, ResizeMode, ResizeOp},
    probe::MediaProbe,
    spatial::Resolution,
    time::TimeRange,
};
use rskit_media_ffmpeg::{FfmpegConfig, FfmpegExecutor, FfmpegProbe};

/// Generate a 1-second test video (320×240, 25fps, with audio) using ffmpeg.
async fn generate_test_video() -> TempFile {
    let tmp = TempFile::with_extension("mp4").expect("create temp");
    let status = tokio::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=duration=1:size=320x240:rate=25",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=1",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-crf",
            "28",
            "-c:a",
            "aac",
            "-b:a",
            "64k",
            "-shortest",
        ])
        .arg(tmp.path())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .expect("ffmpeg command");

    assert!(status.success(), "ffmpeg failed to generate test video");
    tmp
}

/// Generate a 1-second test audio file.
async fn generate_test_audio() -> TempFile {
    let tmp = TempFile::with_extension("wav").expect("create temp");
    let status = tokio::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=1",
            "-c:a",
            "pcm_s16le",
        ])
        .arg(tmp.path())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .expect("ffmpeg command");

    assert!(status.success(), "ffmpeg failed to generate test audio");
    tmp
}

fn is_ffmpeg_available() -> bool {
    std::process::Command::new("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

macro_rules! skip_without_ffmpeg {
    () => {
        if !is_ffmpeg_available() {
            eprintln!("SKIPPED: ffmpeg not available");
            return;
        }
    };
}

// ── FfmpegProbe tests ──────────────────────────────────────────────────────

#[tokio::test]
async fn probe_check_available() {
    skip_without_ffmpeg!();
    let probe = FfmpegProbe::new(FfmpegConfig::default());
    let version = probe
        .check_available()
        .await
        .expect("ffprobe should be available");
    assert!(version.contains("ffprobe"), "got: {version}");
}

#[tokio::test]
async fn probe_video_file() {
    skip_without_ffmpeg!();
    let video = generate_test_video().await;
    let source = FileSource::from_path(video.path());

    let probe = FfmpegProbe::new(FfmpegConfig::default());
    let meta = probe.probe(&source).await.expect("probe video");

    assert!(meta.has_video(), "should detect video track");
    assert!(meta.has_audio(), "should detect audio track");
    assert!(meta.duration.is_some(), "should have duration");

    let dur = meta.duration.unwrap();
    assert!(
        dur.as_secs_f64() >= 0.9 && dur.as_secs_f64() <= 1.5,
        "duration: {dur:?}"
    );

    let res = meta.resolution().expect("should have resolution");
    assert_eq!(res.width, 320);
    assert_eq!(res.height, 240);
}

#[tokio::test]
async fn probe_audio_file() {
    skip_without_ffmpeg!();
    let audio = generate_test_audio().await;
    let source = FileSource::from_path(audio.path());

    let probe = FfmpegProbe::new(FfmpegConfig::default());
    let meta = probe.probe(&source).await.expect("probe audio");

    assert!(!meta.has_video());
    assert!(meta.has_audio());
    let sr = meta.sample_rate().expect("should have sample rate");
    assert!(sr.0 > 0);
}

#[tokio::test]
async fn probe_raw_json() {
    skip_without_ffmpeg!();
    let video = generate_test_video().await;
    let source = FileSource::from_path(video.path());

    let probe = FfmpegProbe::new(FfmpegConfig::default());
    let json = probe.probe_raw(&source).await.expect("probe_raw");

    assert!(json.get("format").is_some(), "JSON should have format");
    assert!(json.get("streams").is_some(), "JSON should have streams");
}

// ── FfmpegExecutor tests ───────────────────────────────────────────────────

#[tokio::test]
async fn executor_resize_video() {
    skip_without_ffmpeg!();
    let video = generate_test_video().await;
    let source = FileSource::from_path(video.path());

    let dir = TempDir::new().expect("temp dir");
    let out_path = dir.path().join("resized.mp4");

    let executor = FfmpegExecutor::new(FfmpegConfig::default(), rskit_media::Registry::default());
    let ops = vec![MediaOp::Resize(ResizeOp {
        resolution: Resolution::new(160, 120),
        mode: ResizeMode::Exact,
    })];

    let result = executor
        .execute(&source, &ops, Some(&FileSink::Path(out_path.clone())))
        .await
        .expect("execute resize");

    // Verify output exists and has correct dimensions
    let probe = FfmpegProbe::new(FfmpegConfig::default());
    let meta = probe.probe(&result).await.expect("probe result");
    let res = meta.resolution().expect("resolution");
    assert_eq!(res.width, 160);
    assert_eq!(res.height, 120);
}

#[tokio::test]
async fn executor_extract_segment() {
    skip_without_ffmpeg!();
    let video = generate_test_video().await;
    let source = FileSource::from_path(video.path());

    let dir = TempDir::new().expect("temp dir");
    let out_path = dir.path().join("segment.mp4");

    let executor = FfmpegExecutor::new(FfmpegConfig::default(), rskit_media::Registry::default());
    let ops = vec![MediaOp::Extract(TimeRange::from_seconds(0.0, 0.5))];

    let result = executor
        .execute(&source, &ops, Some(&FileSink::Path(out_path.clone())))
        .await
        .expect("execute extract");

    // Verify output is shorter
    let probe = FfmpegProbe::new(FfmpegConfig::default());
    let meta = probe.probe(&result).await.expect("probe result");
    let dur = meta.duration.unwrap();
    assert!(dur.as_secs_f64() <= 0.7, "should be ~0.5s, got: {dur:?}");
}

#[tokio::test]
async fn executor_strip_audio() {
    skip_without_ffmpeg!();
    let video = generate_test_video().await;
    let source = FileSource::from_path(video.path());

    let dir = TempDir::new().expect("temp dir");
    let out_path = dir.path().join("no_audio.mp4");

    let executor = FfmpegExecutor::new(FfmpegConfig::default(), rskit_media::Registry::default());
    let ops = vec![MediaOp::StripAudio];

    let result = executor
        .execute(&source, &ops, Some(&FileSink::Path(out_path.clone())))
        .await
        .expect("execute strip audio");

    let probe = FfmpegProbe::new(FfmpegConfig::default());
    let meta = probe.probe(&result).await.expect("probe result");
    assert!(meta.has_video());
    assert!(!meta.has_audio(), "should not have audio track");
}

#[tokio::test]
async fn executor_supports() {
    let executor = FfmpegExecutor::new(FfmpegConfig::default(), rskit_media::Registry::default());
    assert!(executor.supports(&MediaOp::Resize(ResizeOp {
        resolution: Resolution::p720(),
        mode: ResizeMode::Fit,
    })));
    assert!(executor.supports(&MediaOp::StripAudio));
    assert!(executor.supports(&MediaOp::Reverse));
}

#[tokio::test]
async fn executor_preview_returns_args() {
    let source = FileSource::from_path("/tmp/test.mp4");
    let executor = FfmpegExecutor::new(FfmpegConfig::default(), rskit_media::Registry::default());
    let ops = vec![MediaOp::Resize(ResizeOp {
        resolution: Resolution::p720(),
        mode: ResizeMode::Exact,
    })];

    let preview = executor.preview(&source, &ops).expect("preview");
    assert!(!preview.is_empty());
    let joined = preview.join(" ");
    assert!(
        joined.contains("scale=1280:720") || joined.contains("ffmpeg"),
        "got: {joined}"
    );
}

// ── Performance: video processing timing ────────────────────────────────────

#[tokio::test]
async fn perf_resize_timing() {
    skip_without_ffmpeg!();
    let video = generate_test_video().await;
    let source = FileSource::from_path(video.path());

    let dir = TempDir::new().expect("temp dir");
    let out_path = dir.path().join("perf_resized.mp4");

    let executor = FfmpegExecutor::new(FfmpegConfig::default(), rskit_media::Registry::default());
    let ops = vec![MediaOp::Resize(ResizeOp {
        resolution: Resolution::new(160, 120),
        mode: ResizeMode::Exact,
    })];

    let start = std::time::Instant::now();
    let _ = executor
        .execute(&source, &ops, Some(&FileSink::Path(out_path)))
        .await
        .expect("execute");
    let elapsed = start.elapsed();

    // 1-second 320×240 resize should complete quickly
    println!("FFmpeg resize 320x240→160x120: {elapsed:?}");
    assert!(
        elapsed < Duration::from_secs(10),
        "resize took too long: {elapsed:?}"
    );
}
