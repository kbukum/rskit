//! Golden/snapshot tests for rskit-media-ffmpeg using real fixture files.

use std::path::PathBuf;
use std::sync::Arc;

use rskit_media::{
    Registry,
    executor::MediaExecutor,
    filter::filters,
    ops::{MediaOp, ResizeMode, ResizeOp},
    probe::MediaProbe,
    spatial::Resolution,
    subtitle::{SubtitleEntry, SubtitleTrack},
    time::{Segment, TimeRange},
};
use rskit_media_ffmpeg::Config as FfmpegConfig;
use rskit_storage::{FileSink, FileSource, TempDir};

// ── Helpers ──────────────────────────────────────────────────────────────────

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
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

fn has_subtitles_filter() -> bool {
    std::process::Command::new("ffmpeg")
        .args(["-filters", "-v", "quiet"])
        .output()
        .map(|o| {
            let stdout = String::from_utf8_lossy(&o.stdout);
            stdout.contains("subtitles")
        })
        .unwrap_or(false)
}

macro_rules! skip_without_subtitles {
    () => {
        if !has_subtitles_filter() {
            eprintln!("Skipping: ffmpeg subtitles filter not available (needs libass)");
            return;
        }
    };
}

fn ffmpeg_executor() -> Arc<dyn MediaExecutor> {
    let mut registry = Registry::default();
    rskit_media_ffmpeg::register(&mut registry, FfmpegConfig::default())
        .expect("register ffmpeg backend");
    registry.executor("ffmpeg").expect("ffmpeg executor")
}

fn ffmpeg_probe() -> Arc<dyn MediaProbe> {
    let mut registry = Registry::default();
    rskit_media_ffmpeg::register(&mut registry, FfmpegConfig::default())
        .expect("register ffmpeg backend");
    registry.probe("ffmpeg").expect("ffmpeg probe")
}

fn normalized_preview(source: &FileSource, ops: &[MediaOp]) -> Vec<String> {
    ffmpeg_executor()
        .preview(source, ops)
        .expect("preview command")
        .into_iter()
        .map(|line| {
            line.split_whitespace()
                .map(|arg| {
                    if arg.ends_with("/ffmpeg") {
                        "ffmpeg"
                    } else {
                        arg
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

/// Asserts a JSON snapshot with map keys sorted.
///
/// `serde_json::json!` builds a `Value::Object` whose key ordering depends on
/// the `preserve_order` feature: with it enabled keys keep insertion order,
/// without it they are sorted. That feature is turned on transitively by other
/// workspace crates and unified across the whole graph, so the same snapshot
/// would render differently depending on the build scope. Sorting the maps
/// makes these snapshots deterministic regardless of how the build resolves
/// `serde_json`'s features.
fn assert_sorted_json_snapshot(name: &str, value: serde_json::Value) {
    insta::with_settings!({sort_maps => true}, {
        insta::assert_json_snapshot!(name, value);
    });
}

// ── Test 1: Probe real JPEG ──────────────────────────────────────────────────

#[tokio::test]
async fn golden_probe_real_jpeg() {
    skip_without_ffmpeg!();

    let source = FileSource::from_path(fixtures_dir().join("image/ai-generated.jpg"));
    let probe = ffmpeg_probe();
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
    let probe = ffmpeg_probe();
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
    let probe = ffmpeg_probe();
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

    let executor = ffmpeg_executor();
    let ops = vec![MediaOp::Resize(ResizeOp {
        resolution: Resolution::new(100, 100),
        mode: ResizeMode::Exact,
    })];

    let result = executor
        .execute(&source, &ops, Some(&FileSink::Path(out_path.clone())))
        .await
        .expect("execute resize image");

    // Verify output via probe
    let probe = ffmpeg_probe();
    let meta = probe.probe(&result).await.expect("probe resized image");
    let res = meta.resolution().expect("should have resolution");

    assert_sorted_json_snapshot(
        "process_real_image_resize",
        serde_json::json!({
            "output_width": res.width,
            "output_height": res.height,
            "has_video_track": meta.has_video(),
        }),
    );
}

// ── Test 5: Process real audio (extract segment) ─────────────────────────────

#[tokio::test]
async fn golden_process_real_audio() {
    skip_without_ffmpeg!();

    let source = FileSource::from_path(fixtures_dir().join("audio/real-voice.wav"));
    let dir = TempDir::new().expect("temp dir");
    let out_path = dir.path().join("segment.wav");

    let executor = ffmpeg_executor();
    let ops = vec![MediaOp::Extract(TimeRange::from_seconds(0.0, 0.5))];

    let result = executor
        .execute(&source, &ops, Some(&FileSink::Path(out_path.clone())))
        .await
        .expect("execute extract audio");

    let probe = ffmpeg_probe();
    let meta = probe.probe(&result).await.expect("probe extracted audio");

    insta::with_settings!({sort_maps => true}, {
        insta::assert_json_snapshot!("process_real_audio_extract", {
            ".duration_secs" => insta::rounded_redaction(1),
        }, serde_json::json!({
            "has_audio": meta.has_audio(),
            "has_video": meta.has_video(),
            "duration_secs": meta.duration.map(|d| d.as_secs_f64()),
            "sample_rate": meta.sample_rate().map(|sr| sr.0),
        }));
    });
}

// ── Test 6: Process real video (resize) ──────────────────────────────────────

#[tokio::test]
async fn golden_process_real_video() {
    skip_without_ffmpeg!();

    let source = FileSource::from_path(fixtures_dir().join("video/ai-generated.mp4"));
    let dir = TempDir::new().expect("temp dir");
    let out_path = dir.path().join("resized.mp4");

    let executor = ffmpeg_executor();
    let ops = vec![MediaOp::Resize(ResizeOp {
        resolution: Resolution::new(160, 120),
        mode: ResizeMode::Exact,
    })];

    let result = executor
        .execute(&source, &ops, Some(&FileSink::Path(out_path.clone())))
        .await
        .expect("execute resize video");

    let probe = ffmpeg_probe();
    let meta = probe.probe(&result).await.expect("probe resized video");
    let res = meta.resolution().expect("should have resolution");

    insta::with_settings!({sort_maps => true}, {
        insta::assert_json_snapshot!("process_real_video_resize", {
            ".duration_secs" => insta::rounded_redaction(1),
        }, serde_json::json!({
            "output_width": res.width,
            "output_height": res.height,
            "has_video": meta.has_video(),
            "has_audio": meta.has_audio(),
            "duration_secs": meta.duration.map(|d| d.as_secs_f64()),
        }));
    });
}

// ── Test 7: ExtractMany (multi-segment extraction) ───────────────────────────

#[tokio::test]
async fn golden_extract_many() {
    skip_without_ffmpeg!();

    let source = FileSource::from_path(fixtures_dir().join("video/ai-generated.mp4"));
    let dir = TempDir::new().expect("temp dir");
    let out_path = dir.path().join("multi_seg.mp4");

    let executor = ffmpeg_executor();

    // ai-generated.mp4 is ~1.375s — extract two small segments
    let segments = vec![
        Segment::new(TimeRange::from_seconds(0.0, 0.3)),
        Segment::new(TimeRange::from_seconds(0.5, 0.8)),
    ];
    let ops = vec![MediaOp::ExtractMany(segments)];

    let result = executor
        .execute(&source, &ops, Some(&FileSink::Path(out_path.clone())))
        .await
        .expect("execute extract-many");

    let probe = ffmpeg_probe();
    let meta = probe
        .probe(&result)
        .await
        .expect("probe extract-many output");

    insta::with_settings!({sort_maps => true}, {
        insta::assert_json_snapshot!("extract_many_two_segments", {
            ".duration_secs" => insta::rounded_redaction(1),
        }, serde_json::json!({
            "has_video": meta.has_video(),
            "has_audio": meta.has_audio(),
            "duration_secs": meta.duration.map(|d| d.as_secs_f64()),
            "track_count": meta.tracks.len(),
        }));
    });
}

// ── Test 8: Video filter chain (grayscale + blur) ────────────────────────────

#[tokio::test]
async fn golden_filter_chain_video() {
    skip_without_ffmpeg!();

    let source = FileSource::from_path(fixtures_dir().join("video/ai-generated.mp4"));
    let dir = TempDir::new().expect("temp dir");
    let out_path = dir.path().join("filtered.mp4");

    let executor = ffmpeg_executor();
    let ops = vec![
        MediaOp::Filter(filters::grayscale()),
        MediaOp::Filter(filters::blur(2.0)),
    ];

    let result = executor
        .execute(&source, &ops, Some(&FileSink::Path(out_path.clone())))
        .await
        .expect("execute filter chain");

    let probe = ffmpeg_probe();
    let meta = probe.probe(&result).await.expect("probe filtered video");
    let res = meta.resolution().expect("should have resolution");

    insta::with_settings!({sort_maps => true}, {
        insta::assert_json_snapshot!("filter_chain_grayscale_blur", {
            ".duration_secs" => insta::rounded_redaction(1),
        }, serde_json::json!({
            "has_video": meta.has_video(),
            "has_audio": meta.has_audio(),
            "width": res.width,
            "height": res.height,
            "duration_secs": meta.duration.map(|d| d.as_secs_f64()),
        }));
    });
}

// ── Test 9: BurnSubtitles ────────────────────────────────────────────────────

#[tokio::test]
async fn golden_burn_subtitles() {
    skip_without_ffmpeg!();
    skip_without_subtitles!();

    let source = FileSource::from_path(fixtures_dir().join("video/ai-generated.mp4"));
    let dir = TempDir::new().expect("temp dir");
    let out_path = dir.path().join("subtitled.mp4");

    let srt = SubtitleTrack {
        entries: vec![SubtitleEntry {
            range: TimeRange::from_seconds(0.0, 1.0),
            text: "Hello world".into(),
            style: None,
        }],
        language: None,
        default_style: None,
    };

    let executor = ffmpeg_executor();
    let ops = vec![MediaOp::BurnSubtitles(srt)];

    let result = executor
        .execute(&source, &ops, Some(&FileSink::Path(out_path.clone())))
        .await
        .expect("execute burn subtitles");

    let probe = ffmpeg_probe();
    let meta = probe.probe(&result).await.expect("probe subtitled video");
    let res = meta.resolution().expect("should have resolution");

    insta::with_settings!({sort_maps => true}, {
        insta::assert_json_snapshot!("burn_subtitles", {
            ".duration_secs" => insta::rounded_redaction(1),
        }, serde_json::json!({
            "has_video": meta.has_video(),
            "has_audio": meta.has_audio(),
            "width": res.width,
            "height": res.height,
            "duration_secs": meta.duration.map(|d| d.as_secs_f64()),
        }));
    });
}

// ── Test 10: Public preview snapshot (streaming config) ──────────────────────

#[test]
fn golden_preview_hls() {
    use rskit_media::codec::Codec;
    use rskit_media::format::Format;
    use rskit_media::output::{
        Bitrate, EncodingSpeed, HlsConfig, HlsPlaylistType, OutputConfig, Quality, StreamingConfig,
        VideoSettings,
    };

    let source = FileSource::from_path("/dev/null");
    let output = OutputConfig {
        format: Format::new("mp4"),
        video: Some(VideoSettings {
            codec: Codec::new("libx264"),
            resolution: None,
            frame_rate: None,
            quality: Some(Quality::Custom(23)),
            bitrate: Some(Bitrate::Constant(2_000_000)),
            speed: Some(EncodingSpeed::Fast),
            profile: None,
            level: None,
        }),
        audio: None,
        streaming: Some(StreamingConfig::Hls(HlsConfig {
            segment_duration: 4,
            playlist_size: 5,
            playlist_type: HlsPlaylistType::Vod,
            segment_filename: Some("seg_%03d.ts".into()),
        })),
        strip_metadata: false,
        extra: Default::default(),
    };

    let ops = vec![MediaOp::Transcode(output)];
    let preview = normalized_preview(&source, &ops);

    insta::assert_json_snapshot!("preview_hls", &preview);
}

// ── Test 11: Public preview snapshot (extract-many + filters) ────────────────

#[test]
fn golden_preview_extract_many_with_filters() {
    let source = FileSource::from_path("/dev/null");

    let segments = vec![
        Segment::new(TimeRange::from_seconds(0.0, 5.0)),
        Segment::new(TimeRange::from_seconds(10.0, 15.0)),
        Segment::new(TimeRange::from_seconds(20.0, 25.0)),
    ];

    let ops = vec![
        MediaOp::ExtractMany(segments),
        MediaOp::Filter(filters::denoise(3)),
    ];

    let preview = normalized_preview(&source, &ops);

    insta::assert_json_snapshot!("preview_extract_many_filters", &preview);
}

// ── Test 12: Public preview verifies operation optimization ──────────────────

#[test]
fn golden_preview_optimized_ops() {
    // Consecutive resizes — only last should survive
    let ops = vec![
        MediaOp::Resize(ResizeOp {
            resolution: Resolution::new(1920, 1080),
            mode: ResizeMode::Exact,
        }),
        MediaOp::Resize(ResizeOp {
            resolution: Resolution::new(1280, 720),
            mode: ResizeMode::Fit,
        }),
        MediaOp::Volume(0.5),
        MediaOp::Volume(0.8),
        MediaOp::Speed(2.0),
        MediaOp::Speed(1.0), // no-op, should be folded in
        MediaOp::Filter(filters::grayscale()),
    ];

    let source = FileSource::from_path("/dev/null");
    let preview = normalized_preview(&source, &ops);

    insta::assert_json_snapshot!("preview_optimized_ops", &preview);
}
