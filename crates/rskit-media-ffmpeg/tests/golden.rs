//! Golden/snapshot tests for rskit-media-ffmpeg using real fixture files.

use std::path::PathBuf;

use rskit_file::{FileSink, FileSource, TempDir};
use rskit_media::{
    executor::MediaExecutor,
    ops::{MediaOp, ResizeMode, ResizeOp},
    probe::MediaProbe,
    spatial::Resolution,
    time::TimeRange,
};
use rskit_media_ffmpeg::{FfmpegConfig, FfmpegExecutor, FfmpegProbe};

// ── Helpers ──────────────────────────────────────────────────────────────────

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures")
}

fn has_ffmpeg() -> bool {
    std::process::Command::new("ffprobe")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

macro_rules! skip_without_ffmpeg {
    () => {
        if !has_ffmpeg() {
            eprintln!("Skipping: ffprobe not found");
            return;
        }
    };
}

// ── Test 1: Probe real JPEG ──────────────────────────────────────────────────

#[tokio::test]
async fn golden_probe_real_jpeg() {
    skip_without_ffmpeg!();

    let source = FileSource::from_path(fixtures_dir().join("image/ai-generated.jpg"));
    let probe = FfmpegProbe::new(FfmpegConfig::default());
    let meta = probe.probe(&source).await.expect("probe JPEG");

    insta::assert_json_snapshot!("probe_real_jpeg", {
        ".duration" => "[duration]",
        ".size" => "[size]",
        ".bitrate" => "[bitrate]",
        ".tags" => "[tags]",
        ".created_at" => "[created_at]",
        ".tracks[].bitrate" => "[bitrate]",
        ".tracks[].duration" => "[duration]",
    }, &meta);
}

// ── Test 2: Probe real WAV ───────────────────────────────────────────────────

#[tokio::test]
async fn golden_probe_real_wav() {
    skip_without_ffmpeg!();

    let source = FileSource::from_path(fixtures_dir().join("audio/ai-generated.wav"));
    let probe = FfmpegProbe::new(FfmpegConfig::default());
    let meta = probe.probe(&source).await.expect("probe WAV");

    insta::assert_json_snapshot!("probe_real_wav", {
        ".duration" => "[duration]",
        ".size" => "[size]",
        ".bitrate" => "[bitrate]",
        ".tags" => "[tags]",
        ".created_at" => "[created_at]",
        ".tracks[].bitrate" => "[bitrate]",
        ".tracks[].duration" => "[duration]",
    }, &meta);
}

// ── Test 3: Probe real MP4 ───────────────────────────────────────────────────

#[tokio::test]
async fn golden_probe_real_mp4() {
    skip_without_ffmpeg!();

    let source = FileSource::from_path(fixtures_dir().join("video/ai-generated.mp4"));
    let probe = FfmpegProbe::new(FfmpegConfig::default());
    let meta = probe.probe(&source).await.expect("probe MP4");

    insta::assert_json_snapshot!("probe_real_mp4", {
        ".duration" => "[duration]",
        ".size" => "[size]",
        ".bitrate" => "[bitrate]",
        ".tags" => "[tags]",
        ".created_at" => "[created_at]",
        ".tracks[].bitrate" => "[bitrate]",
        ".tracks[].duration" => "[duration]",
    }, &meta);
}

// ── Test 4: Process real image (resize) ──────────────────────────────────────

#[tokio::test]
async fn golden_process_real_image() {
    skip_without_ffmpeg!();

    let source = FileSource::from_path(fixtures_dir().join("image/ai-generated.jpg"));
    let dir = TempDir::new().expect("temp dir");
    let out_path = dir.path().join("resized.jpg");

    let executor = FfmpegExecutor::new(FfmpegConfig::default(), rskit_media::Registry::default());
    let ops = vec![MediaOp::Resize(ResizeOp {
        resolution: Resolution::new(100, 100),
        mode: ResizeMode::Exact,
    })];

    let result = executor
        .execute(&source, &ops, Some(&FileSink::Path(out_path.clone())))
        .await
        .expect("execute resize image");

    // Verify output via probe
    let probe = FfmpegProbe::new(FfmpegConfig::default());
    let meta = probe.probe(&result).await.expect("probe resized image");
    let res = meta.resolution().expect("should have resolution");

    insta::assert_json_snapshot!("process_real_image_resize", serde_json::json!({
        "output_width": res.width,
        "output_height": res.height,
        "has_video_track": meta.has_video(),
    }));
}

// ── Test 5: Process real audio (extract segment) ─────────────────────────────

#[tokio::test]
async fn golden_process_real_audio() {
    skip_without_ffmpeg!();

    let source = FileSource::from_path(fixtures_dir().join("audio/real-voice.wav"));
    let dir = TempDir::new().expect("temp dir");
    let out_path = dir.path().join("segment.wav");

    let executor = FfmpegExecutor::new(FfmpegConfig::default(), rskit_media::Registry::default());
    let ops = vec![MediaOp::Extract(TimeRange::from_seconds(0.0, 0.5))];

    let result = executor
        .execute(&source, &ops, Some(&FileSink::Path(out_path.clone())))
        .await
        .expect("execute extract audio");

    let probe = FfmpegProbe::new(FfmpegConfig::default());
    let meta = probe.probe(&result).await.expect("probe extracted audio");

    insta::assert_json_snapshot!("process_real_audio_extract", {
        ".duration_secs" => insta::rounded_redaction(1),
    }, serde_json::json!({
        "has_audio": meta.has_audio(),
        "has_video": meta.has_video(),
        "duration_secs": meta.duration.map(|d| d.as_secs_f64()),
        "sample_rate": meta.sample_rate().map(|sr| sr.0),
    }));
}

// ── Test 6: Process real video (resize) ──────────────────────────────────────

#[tokio::test]
async fn golden_process_real_video() {
    skip_without_ffmpeg!();

    let source = FileSource::from_path(fixtures_dir().join("video/ai-generated.mp4"));
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
        .expect("execute resize video");

    let probe = FfmpegProbe::new(FfmpegConfig::default());
    let meta = probe.probe(&result).await.expect("probe resized video");
    let res = meta.resolution().expect("should have resolution");

    insta::assert_json_snapshot!("process_real_video_resize", {
        ".duration_secs" => insta::rounded_redaction(1),
    }, serde_json::json!({
        "output_width": res.width,
        "output_height": res.height,
        "has_video": meta.has_video(),
        "has_audio": meta.has_audio(),
        "duration_secs": meta.duration.map(|d| d.as_secs_f64()),
    }));
}
