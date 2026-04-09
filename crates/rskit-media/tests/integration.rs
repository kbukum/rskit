//! Comprehensive tests for rskit-media types, registry, pipeline, and parsing.

use std::time::Duration;

use rskit_media::*;

// ── Timestamp ───────────────────────────────────────────────────────────────

#[test]
fn timestamp_from_millis() {
    let ts = Timestamp::from_millis(1500);
    assert_eq!(ts.as_millis(), 1500);
    assert!((ts.as_seconds() - 1.5).abs() < f64::EPSILON);
}

#[test]
fn timestamp_from_seconds() {
    let ts = Timestamp::from_seconds(2.5);
    assert_eq!(ts.as_millis(), 2500);
}

#[test]
fn timestamp_from_hms() {
    let ts = Timestamp::from_hms(1, 30, 15.5);
    // 1h30m15.5s = 5415500ms
    assert_eq!(ts.as_millis(), 5415500);
}

#[test]
fn timestamp_as_duration() {
    let ts = Timestamp::from_millis(3000);
    assert_eq!(ts.as_duration(), Duration::from_secs(3));
}

#[test]
fn timestamp_to_ffmpeg_time() {
    let ts = Timestamp::from_hms(1, 2, 3.456);
    let ffmpeg = ts.to_ffmpeg_time();
    assert!(ffmpeg.starts_with("01:02:03"), "got: {ffmpeg}");
}

#[test]
fn timestamp_ordering() {
    let a = Timestamp::from_millis(100);
    let b = Timestamp::from_millis(200);
    assert!(a < b);
    assert_eq!(a, Timestamp::from_millis(100));
}

// ── TimeRange ───────────────────────────────────────────────────────────────

#[test]
fn time_range_duration() {
    let r = TimeRange::from_millis(1000, 3500);
    assert_eq!(r.duration_ms(), 2500);
    assert_eq!(r.duration(), Duration::from_millis(2500));
}

#[test]
fn time_range_contains() {
    let r = TimeRange::from_seconds(1.0, 5.0);
    assert!(r.contains(Timestamp::from_seconds(3.0)));
    assert!(!r.contains(Timestamp::from_seconds(0.5)));
    assert!(!r.contains(Timestamp::from_seconds(6.0)));
}

#[test]
fn time_range_overlaps() {
    let a = TimeRange::from_seconds(1.0, 5.0);
    let b = TimeRange::from_seconds(3.0, 7.0);
    let c = TimeRange::from_seconds(6.0, 8.0);
    assert!(a.overlaps(&b));
    assert!(!a.overlaps(&c));
}

#[test]
fn time_range_merge() {
    let a = TimeRange::from_millis(0, 3000);
    let b = TimeRange::from_millis(2000, 5000);
    let merged = a.merge(&b).expect("should merge overlapping ranges");
    assert_eq!(merged.start, Timestamp::from_millis(0));
    assert_eq!(merged.end, Timestamp::from_millis(5000));
}

#[test]
fn time_range_merge_non_overlapping() {
    let a = TimeRange::from_millis(0, 1000);
    let b = TimeRange::from_millis(2000, 3000);
    assert!(a.merge(&b).is_none());
}

#[test]
fn time_range_split_at() {
    let r = TimeRange::from_millis(0, 4000);
    let (left, right) = r.split_at(Timestamp::from_millis(2000));
    let left = left.expect("should have left");
    let right = right.expect("should have right");
    assert_eq!(left.end, Timestamp::from_millis(2000));
    assert_eq!(right.start, Timestamp::from_millis(2000));
}

#[test]
fn time_range_shift() {
    let r = TimeRange::from_millis(1000, 3000);
    let shifted = r.shift(500);
    assert_eq!(shifted.start, Timestamp::from_millis(1500));
    assert_eq!(shifted.end, Timestamp::from_millis(3500));
}

// ── Segment ─────────────────────────────────────────────────────────────────

#[test]
fn segment_builder() {
    let seg = time::Segment::new(TimeRange::from_millis(0, 5000))
        .with_label("intro")
        .with_confidence(0.95);
    assert_eq!(seg.label.as_deref(), Some("intro"));
    assert!((seg.confidence.unwrap() - 0.95).abs() < f32::EPSILON);
}

// ── Resolution ──────────────────────────────────────────────────────────────

#[test]
fn resolution_presets() {
    assert_eq!(Resolution::p360(), Resolution::new(640, 360));
    assert_eq!(Resolution::p720(), Resolution::new(1280, 720));
    assert_eq!(Resolution::p1080(), Resolution::new(1920, 1080));
    assert_eq!(Resolution::p4k(), Resolution::new(3840, 2160));
}

#[test]
fn resolution_aspect_ratio() {
    let r = Resolution::new(1920, 1080);
    let (w, h) = r.aspect_ratio();
    // 16:9
    assert_eq!(w, 16);
    assert_eq!(h, 9);
}

#[test]
fn resolution_orientation() {
    assert!(Resolution::new(1920, 1080).is_landscape());
    assert!(Resolution::new(1080, 1920).is_portrait());
    assert!(Resolution::new(500, 500).is_square());
}

#[test]
fn resolution_pixel_count() {
    let r = Resolution::p1080();
    assert_eq!(r.pixel_count(), 1920 * 1080);
}

#[test]
fn resolution_scale_to_fit() {
    let r = Resolution::new(1920, 1080);
    let scaled = r.scale_to_fit(640, 480);
    assert!(scaled.width <= 640);
    assert!(scaled.height <= 480);
    // Should maintain 16:9 aspect ratio
    let ar = scaled.width as f64 / scaled.height as f64;
    assert!((ar - 16.0 / 9.0).abs() < 0.1);
}

#[test]
fn resolution_scale_by() {
    let r = Resolution::new(100, 200);
    let scaled = r.scale_by(2.0);
    assert_eq!(scaled.width, 200);
    assert_eq!(scaled.height, 400);
}

// ── FrameRate ───────────────────────────────────────────────────────────────

#[test]
fn frame_rate_presets() {
    assert_eq!(FrameRate::fps_24(), FrameRate::new(24, 1));
    assert_eq!(FrameRate::fps_30(), FrameRate::new(30, 1));
    assert_eq!(FrameRate::fps_60(), FrameRate::new(60, 1));
}

#[test]
fn frame_rate_ntsc() {
    let ntsc = FrameRate::ntsc_30();
    assert_eq!(ntsc.num, 30000);
    assert_eq!(ntsc.den, 1001);
    let fps = ntsc.as_f64();
    assert!((fps - 29.97).abs() < 0.01);
}

#[test]
fn frame_rate_as_f64() {
    assert!((FrameRate::fps(24).as_f64() - 24.0).abs() < f64::EPSILON);
}

// ── Codec & Format ──────────────────────────────────────────────────────────

#[test]
fn codec_creation_and_equality() {
    let c1 = Codec::new("h264");
    let c2 = Codec::new("h264");
    let c3 = Codec::new("h265");
    assert_eq!(c1, c2);
    assert_ne!(c1, c3);
    assert_eq!(c1.id(), "h264");
}

#[test]
fn codec_well_known_constants() {
    // Video codecs
    assert_eq!(codec::video::H264, "h264");
    assert_eq!(codec::video::H265, "h265");
    assert_eq!(codec::video::VP9, "vp9");
    assert_eq!(codec::video::AV1, "av1");
    // Audio codecs
    assert_eq!(codec::audio::AAC, "aac");
    assert_eq!(codec::audio::OPUS, "opus");
    assert_eq!(codec::audio::MP3, "mp3");
}

#[test]
fn format_creation_and_equality() {
    let f1 = Format::new("mp4");
    let f2 = Format::new("mp4");
    let f3 = Format::new("webm");
    assert_eq!(f1, f2);
    assert_ne!(f1, f3);
    assert_eq!(f1.id(), "mp4");
}

// ── Registry ────────────────────────────────────────────────────────────────

#[test]
fn registry_default_has_common_codecs() {
    let reg = Registry::default();
    let h264_info = reg.codec_info(&Codec::new("h264"));
    assert!(h264_info.is_some(), "registry should have h264");
    let info = h264_info.unwrap();
    assert_eq!(info.kind, CodecKind::Video);
}

#[test]
fn registry_default_has_common_formats() {
    let reg = Registry::default();
    let mp4_info = reg.format_info(&Format::new("mp4"));
    assert!(mp4_info.is_some(), "registry should have mp4");
    let info = mp4_info.unwrap();
    assert_eq!(info.extension, "mp4");
    assert!(info.is_container);
}

#[test]
fn registry_compatibility_h264_mp4() {
    let reg = Registry::default();
    let h264 = Codec::new("h264");
    let mp4 = Format::new("mp4");
    assert!(
        reg.is_compatible(&h264, &mp4),
        "h264 should be compatible with mp4"
    );
}

#[test]
fn registry_compatibility_vp9_webm() {
    let reg = Registry::default();
    assert!(reg.is_compatible(&Codec::new("vp9"), &Format::new("webm")));
}

#[test]
fn registry_incompatible_codecs() {
    let reg = Registry::default();
    // VP9 is not typically compatible with MP3 container
    assert!(!reg.is_compatible(&Codec::new("vp9"), &Format::new("mp3")));
}

#[test]
fn registry_custom_codec() {
    let mut reg = Registry::default();
    reg.register_codec(registry::CodecInfo {
        id: Codec::new("custom_codec"),
        kind: CodecKind::Video,
        display_name: "Custom".into(),
        ffmpeg_encoder: Some("libcustom".into()),
        ffmpeg_decoder: None,
        compatible_formats: vec![Format::new("mp4")],
    });
    assert!(reg.is_compatible(&Codec::new("custom_codec"), &Format::new("mp4")));
}

// ── Filter ──────────────────────────────────────────────────────────────────

#[test]
fn filter_creation() {
    let f = Filter {
        name: "denoise".into(),
        target: FilterTarget::Video,
        params: Params::new(),
    };
    assert_eq!(f.name, "denoise");
    assert_eq!(f.target, FilterTarget::Video);
}

#[test]
fn filter_with_params() {
    let params = Params::new()
        .set("strength", ParamValue::Int(5))
        .set("radius", ParamValue::Float(1.5))
        .set("enabled", ParamValue::Bool(true));

    assert!(matches!(params.get("strength"), Some(ParamValue::Int(5))));
    assert!(matches!(
        params.get("enabled"),
        Some(ParamValue::Bool(true))
    ));
}

// ── OutputConfig & Presets ──────────────────────────────────────────────────

#[test]
fn preset_mp4_h264() {
    let config = presets::mp4_h264();
    assert_eq!(config.format, Format::new("mp4"));
    assert!(config.video.is_some());
    assert!(config.audio.is_some());
    let video = config.video.as_ref().unwrap();
    assert_eq!(video.codec, Codec::new("h264"));
}

#[test]
fn preset_webm_vp9() {
    let config = presets::webm_vp9();
    assert_eq!(config.format, Format::new("webm"));
    let video = config.video.as_ref().unwrap();
    assert_eq!(video.codec, Codec::new("vp9"));
}

#[test]
fn preset_mp3() {
    let config = presets::mp3();
    assert_eq!(config.format, Format::new("mp3"));
    assert!(config.video.is_none());
    assert!(config.audio.is_some());
}

#[test]
fn preset_wav() {
    let config = presets::wav();
    assert_eq!(config.format, Format::new("wav"));
    assert!(config.audio.is_some());
}

#[test]
fn preset_png() {
    let config = presets::png();
    assert_eq!(config.format, Format::new("png"));
}

// ── MediaOp & ops::spatial ──────────────────────────────────────────────────

#[test]
fn crop_region_new() {
    let c = ops::CropRegion::new(10, 20, 100, 200);
    assert_eq!(c.x, 10);
    assert_eq!(c.y, 20);
    assert_eq!(c.width, 100);
    assert_eq!(c.height, 200);
}

#[test]
fn crop_region_center() {
    let src = Resolution::new(1920, 1080);
    let crop = ops::CropRegion::center(src, 640, 480);
    assert_eq!(crop.width, 640);
    assert_eq!(crop.height, 480);
    assert_eq!(crop.x, (1920 - 640) / 2);
    assert_eq!(crop.y, (1080 - 480) / 2);
}

#[test]
fn crop_region_center_aspect() {
    let src = Resolution::new(1920, 1080);
    let crop = ops::CropRegion::center_aspect(src, 1, 1);
    // Square crop from 16:9 → should be 1080×1080
    assert_eq!(crop.width, crop.height);
    assert_eq!(crop.height, 1080);
}

// ── Pipeline builder ────────────────────────────────────────────────────────

#[test]
fn pipeline_builder_chaining() {
    let source = rskit_file::FileSource::from_path("/tmp/test.mp4");
    let pipe = pipeline::MediaPipeline::from(&source)
        .resize(Resolution::p720(), ops::ResizeMode::Fit)
        .crop(ops::CropRegion::new(0, 0, 640, 360))
        .rotate(ops::Rotation::Degrees90)
        .flip(ops::FlipDirection::Horizontal)
        .speed(2.0)
        .volume(0.5);

    let ops_list = pipe.operations();
    assert_eq!(ops_list.len(), 6);
    assert!(matches!(ops_list[0], MediaOp::Resize(_)));
    assert!(matches!(ops_list[1], MediaOp::Crop(_)));
    assert!(matches!(ops_list[2], MediaOp::Rotate(_)));
    assert!(matches!(ops_list[3], MediaOp::Flip(_)));
    assert!(matches!(ops_list[4], MediaOp::Speed(_)));
    assert!(matches!(ops_list[5], MediaOp::Volume(_)));
}

#[test]
fn pipeline_extract() {
    let source = rskit_file::FileSource::from_path("/tmp/test.mp4");
    let pipe = pipeline::MediaPipeline::from(&source).extract(TimeRange::from_seconds(1.0, 5.0));
    assert_eq!(pipe.operations().len(), 1);
    assert!(matches!(pipe.operations()[0], MediaOp::Extract(_)));
}

#[test]
fn pipeline_transcode() {
    let source = rskit_file::FileSource::from_path("/tmp/test.mp4");
    let pipe = pipeline::MediaPipeline::from(&source).transcode(presets::webm_vp9());
    assert_eq!(pipe.operations().len(), 1);
    assert!(matches!(pipe.operations()[0], MediaOp::Transcode(_)));
}

// ── Subtitle SRT parsing (golden tests) ─────────────────────────────────────

const SRT_FIXTURE: &str = "\
1
00:00:01,000 --> 00:00:04,000
Hello, world!

2
00:00:05,500 --> 00:00:08,200
This is a subtitle test.
With a second line.

3
00:00:10,000 --> 00:00:12,500
Final subtitle.
";

#[test]
fn srt_parse_entries() {
    let track = SubtitleTrack::from_srt(SRT_FIXTURE).expect("valid SRT");
    assert_eq!(track.entries.len(), 3);
}

#[test]
fn srt_parse_first_entry_timing() {
    let track = SubtitleTrack::from_srt(SRT_FIXTURE).unwrap();
    let first = &track.entries[0];
    assert_eq!(first.range.start, Timestamp::from_millis(1000));
    assert_eq!(first.range.end, Timestamp::from_millis(4000));
    assert_eq!(first.text, "Hello, world!");
}

#[test]
fn srt_parse_multiline_entry() {
    let track = SubtitleTrack::from_srt(SRT_FIXTURE).unwrap();
    let second = &track.entries[1];
    assert!(second.text.contains("second line"), "got: {}", second.text);
    assert_eq!(second.range.start, Timestamp::from_millis(5500));
    assert_eq!(second.range.end, Timestamp::from_millis(8200));
}

#[test]
fn srt_roundtrip() {
    let track = SubtitleTrack::from_srt(SRT_FIXTURE).unwrap();
    let srt_output = track.to_srt();
    // Re-parse the output
    let track2 = SubtitleTrack::from_srt(&srt_output).expect("should re-parse SRT output");
    assert_eq!(track.entries.len(), track2.entries.len());
    for (a, b) in track.entries.iter().zip(track2.entries.iter()) {
        assert_eq!(a.range.start, b.range.start);
        assert_eq!(a.range.end, b.range.end);
        assert_eq!(a.text, b.text);
    }
}

// ── Subtitle VTT parsing (golden tests) ─────────────────────────────────────

const VTT_FIXTURE: &str = "\
WEBVTT

00:00:01.000 --> 00:00:04.000
Hello, world!

00:00:05.500 --> 00:00:08.200
VTT subtitle test.

NOTE This is a comment

00:00:10.000 --> 00:00:12.500
Final entry.
";

#[test]
fn vtt_parse_entries() {
    let track = SubtitleTrack::from_vtt(VTT_FIXTURE).expect("valid VTT");
    assert_eq!(track.entries.len(), 3, "should skip comments");
}

#[test]
fn vtt_parse_first_entry() {
    let track = SubtitleTrack::from_vtt(VTT_FIXTURE).unwrap();
    let first = &track.entries[0];
    assert_eq!(first.range.start, Timestamp::from_millis(1000));
    assert_eq!(first.range.end, Timestamp::from_millis(4000));
    assert_eq!(first.text, "Hello, world!");
}

// ── MediaType & TrackKind ───────────────────────────────────────────────────

#[test]
fn media_type_variants() {
    let _video = MediaType::Video;
    let _audio = MediaType::Audio;
    let _image = MediaType::Image;
}

#[test]
fn track_kind_variants() {
    let _v = TrackKind::Video;
    let _a = TrackKind::Audio;
    let _s = TrackKind::Subtitle;
}

// ── SampleRate & ChannelLayout ──────────────────────────────────────────────

#[test]
fn sample_rate_values() {
    let sr = audio::SampleRate(44100);
    assert_eq!(sr.0, 44100);
}

#[test]
fn channel_layout_count() {
    assert_eq!(audio::ChannelLayout::Mono.channel_count(), 1);
    assert_eq!(audio::ChannelLayout::Stereo.channel_count(), 2);
    assert_eq!(audio::ChannelLayout::Surround51.channel_count(), 6);
}

// ── Serde roundtrip tests ───────────────────────────────────────────────────

#[test]
fn timestamp_serde_roundtrip() {
    let ts = Timestamp::from_millis(12345);
    let json = serde_json::to_string(&ts).unwrap();
    let ts2: Timestamp = serde_json::from_str(&json).unwrap();
    assert_eq!(ts, ts2);
}

#[test]
fn resolution_serde_roundtrip() {
    let r = Resolution::p1080();
    let json = serde_json::to_string(&r).unwrap();
    let r2: Resolution = serde_json::from_str(&json).unwrap();
    assert_eq!(r, r2);
}

#[test]
fn codec_serde_roundtrip() {
    let c = Codec::new("h264");
    let json = serde_json::to_string(&c).unwrap();
    let c2: Codec = serde_json::from_str(&json).unwrap();
    assert_eq!(c, c2);
}

#[test]
fn format_serde_roundtrip() {
    let f = Format::new("mp4");
    let json = serde_json::to_string(&f).unwrap();
    let f2: Format = serde_json::from_str(&json).unwrap();
    assert_eq!(f, f2);
}

// ════════════════════════════════════════════════════════════════════════
// NEW TDD TESTS
// ════════════════════════════════════════════════════════════════════════

// ── 1. Timestamp edge cases ─────────────────────────────────────────────

#[test]
fn timestamp_zero() {
    let ts = Timestamp::from_millis(0);
    assert_eq!(ts.as_millis(), 0);
    assert!((ts.as_seconds() - 0.0).abs() < f64::EPSILON);
    assert_eq!(ts.to_ffmpeg_time(), "00:00:00.000");
}

#[test]
fn timestamp_large_value() {
    // u64::MAX / 1000 is the maximum millisecond value that fits in μs storage
    let big = u64::MAX / 1000;
    let ts = Timestamp::from_millis(big);
    assert_eq!(ts.as_millis(), big);
    // Should not panic
    let _ = ts.as_seconds();
    let _ = ts.to_ffmpeg_time();
    let _ = ts.as_duration();
}

#[test]
fn timestamp_large_value_saturates() {
    // Values that would overflow μs storage are saturated to u64::MAX
    let huge = u64::MAX / 2;
    let ts = Timestamp::from_millis(huge);
    assert_eq!(ts.as_micros(), u64::MAX); // saturated
}

#[test]
fn timestamp_from_hms_zero() {
    let ts = Timestamp::from_hms(0, 0, 0.0);
    assert_eq!(ts.as_millis(), 0);
}

#[test]
fn timestamp_display_trait() {
    let ts = Timestamp::from_hms(1, 2, 3.456);
    let display = format!("{}", ts);
    assert_eq!(display, ts.to_ffmpeg_time());
}

#[test]
fn timestamp_hms_fractional() {
    let ts = Timestamp::from_hms(0, 0, 0.001);
    assert_eq!(ts.as_millis(), 1);
}

// ── 2. TimeRange edge cases ─────────────────────────────────────────────

#[test]
fn time_range_zero_duration() {
    let r = TimeRange::from_millis(5000, 5000);
    assert_eq!(r.duration_ms(), 0);
    assert_eq!(r.duration(), Duration::from_millis(0));
}

#[test]
fn time_range_contains_boundary() {
    let r = TimeRange::from_millis(1000, 5000);
    assert!(
        r.contains(Timestamp::from_millis(1000)),
        "should contain start"
    );
    assert!(
        r.contains(Timestamp::from_millis(5000)),
        "should contain end"
    );
}

#[test]
fn time_range_shift_negative() {
    let r = TimeRange::from_millis(5000, 10000);
    let shifted = r.shift(-2000);
    assert_eq!(shifted.start, Timestamp::from_millis(3000));
    assert_eq!(shifted.end, Timestamp::from_millis(8000));
}

#[test]
fn time_range_shift_saturating() {
    let r = TimeRange::from_millis(1000, 3000);
    let shifted = r.shift(-5000);
    assert_eq!(shifted.start, Timestamp::from_millis(0));
    assert_eq!(shifted.end, Timestamp::from_millis(0));
}

#[test]
fn time_range_split_at_start() {
    let r = TimeRange::from_millis(1000, 5000);
    let (left, right) = r.split_at(Timestamp::from_millis(1000));
    assert!(left.is_none());
    assert!(right.is_some());
    let right = right.unwrap();
    assert_eq!(right.start, Timestamp::from_millis(1000));
    assert_eq!(right.end, Timestamp::from_millis(5000));
}

#[test]
fn time_range_split_at_end() {
    let r = TimeRange::from_millis(1000, 5000);
    let (left, right) = r.split_at(Timestamp::from_millis(5000));
    assert!(left.is_some());
    assert!(right.is_none());
    let left = left.unwrap();
    assert_eq!(left.start, Timestamp::from_millis(1000));
    assert_eq!(left.end, Timestamp::from_millis(5000));
}

#[test]
fn time_range_from_seconds() {
    let r = TimeRange::from_seconds(1.5, 3.5);
    assert_eq!(r.start, Timestamp::from_millis(1500));
    assert_eq!(r.end, Timestamp::from_millis(3500));
    assert_eq!(r.duration_ms(), 2000);
}

#[test]
fn time_range_serde_roundtrip() {
    let r = TimeRange::from_millis(1234, 5678);
    let json = serde_json::to_string(&r).unwrap();
    let r2: TimeRange = serde_json::from_str(&json).unwrap();
    assert_eq!(r, r2);
}

// ── 3. Resolution edge cases ────────────────────────────────────────────

#[test]
fn resolution_zero_dimensions() {
    let r = Resolution::new(0, 0);
    let (w, h) = r.aspect_ratio();
    assert_eq!(w, 0);
    assert_eq!(h, 0);
    assert!((r.aspect_ratio_f64() - 0.0).abs() < f64::EPSILON);
}

#[test]
fn resolution_aspect_ratio_f64() {
    let r16_9 = Resolution::new(1920, 1080);
    assert!((r16_9.aspect_ratio_f64() - 16.0 / 9.0).abs() < 0.01);

    let square = Resolution::new(500, 500);
    assert!((square.aspect_ratio_f64() - 1.0).abs() < f64::EPSILON);

    let r4_3 = Resolution::new(640, 480);
    assert!((r4_3.aspect_ratio_f64() - 4.0 / 3.0).abs() < 0.01);
}

#[test]
fn resolution_p480_preset() {
    assert_eq!(Resolution::p480(), Resolution::new(854, 480));
}

#[test]
fn resolution_p1440_preset() {
    assert_eq!(Resolution::p1440(), Resolution::new(2560, 1440));
}

#[test]
fn resolution_scale_to_fill() {
    let r = Resolution::new(1920, 1080);
    let filled = r.scale_to_fill(640, 480);
    // scale_to_fill uses max ratio, so result should cover target entirely
    assert!(filled.width >= 640);
    assert!(filled.height >= 480);
}

#[test]
fn resolution_scale_by_fractional() {
    let r = Resolution::new(100, 200);
    let scaled = r.scale_by(0.5);
    assert_eq!(scaled.width, 50);
    assert_eq!(scaled.height, 100);
}

#[test]
fn resolution_serde_all_presets() {
    for res in [
        Resolution::p360(),
        Resolution::p480(),
        Resolution::p720(),
        Resolution::p1080(),
        Resolution::p1440(),
        Resolution::p4k(),
    ] {
        let json = serde_json::to_string(&res).unwrap();
        let res2: Resolution = serde_json::from_str(&json).unwrap();
        assert_eq!(res, res2);
    }
}

// ── 4. FrameRate edge cases ─────────────────────────────────────────────

#[test]
fn frame_rate_fps_25() {
    let fr = FrameRate::fps_25();
    assert_eq!(fr, FrameRate::new(25, 1));
    assert!((fr.as_f64() - 25.0).abs() < f64::EPSILON);
}

#[test]
fn frame_rate_fps_50() {
    let fr = FrameRate::fps_50();
    assert_eq!(fr, FrameRate::new(50, 1));
    assert!((fr.as_f64() - 50.0).abs() < f64::EPSILON);
}

#[test]
fn frame_rate_ntsc_24() {
    let fr = FrameRate::ntsc_24();
    assert_eq!(fr.num, 24000);
    assert_eq!(fr.den, 1001);
    assert!((fr.as_f64() - 23.976).abs() < 0.01);
}

#[test]
fn frame_rate_ntsc_60() {
    let fr = FrameRate::ntsc_60();
    assert_eq!(fr.num, 60000);
    assert_eq!(fr.den, 1001);
    assert!((fr.as_f64() - 59.94).abs() < 0.01);
}

#[test]
fn frame_rate_zero_denominator() {
    let fr = FrameRate::new(30, 0);
    assert!((fr.as_f64() - 0.0).abs() < f64::EPSILON);
}

#[test]
fn frame_rate_serde_roundtrip() {
    for fr in [
        FrameRate::fps_24(),
        FrameRate::fps_30(),
        FrameRate::fps_60(),
        FrameRate::ntsc_30(),
    ] {
        let json = serde_json::to_string(&fr).unwrap();
        let fr2: FrameRate = serde_json::from_str(&json).unwrap();
        assert_eq!(fr, fr2);
    }
}

// ── 5. Codec and Format ─────────────────────────────────────────────────

#[test]
fn codec_display_trait() {
    let c = Codec::new("h264");
    assert_eq!(format!("{}", c), "h264");
}

#[test]
fn format_display_trait() {
    let f = Format::new("mp4");
    assert_eq!(format!("{}", f), "mp4");
}

#[test]
fn codec_all_video_constants() {
    let expected = [
        (codec::video::H264, "h264"),
        (codec::video::H265, "h265"),
        (codec::video::VP8, "vp8"),
        (codec::video::VP9, "vp9"),
        (codec::video::AV1, "av1"),
        (codec::video::PRORES, "prores"),
        (codec::video::MPEG2, "mpeg2"),
        (codec::video::MPEG4, "mpeg4"),
        (codec::video::THEORA, "theora"),
        (codec::video::WMV3, "wmv3"),
    ];
    for (constant, value) in expected {
        assert_eq!(constant, value);
    }
}

#[test]
fn codec_all_audio_constants() {
    let expected = [
        (codec::audio::AAC, "aac"),
        (codec::audio::OPUS, "opus"),
        (codec::audio::MP3, "mp3"),
        (codec::audio::FLAC, "flac"),
        (codec::audio::VORBIS, "vorbis"),
        (codec::audio::PCM, "pcm"),
        (codec::audio::AC3, "ac3"),
        (codec::audio::EAC3, "eac3"),
        (codec::audio::WMA, "wma"),
        (codec::audio::ALAC, "alac"),
    ];
    for (constant, value) in expected {
        assert_eq!(constant, value);
    }
}

#[test]
fn codec_all_image_constants() {
    let expected = [
        (codec::image::PNG, "png"),
        (codec::image::JPEG, "jpeg"),
        (codec::image::WEBP, "webp"),
        (codec::image::GIF, "gif"),
        (codec::image::BMP, "bmp"),
        (codec::image::TIFF, "tiff"),
        (codec::image::AVIF, "avif"),
        (codec::image::HEIF, "heif"),
    ];
    for (constant, value) in expected {
        assert_eq!(constant, value);
    }
}

#[test]
fn codec_subtitle_constants() {
    let expected = [
        (codec::subtitle::SRT, "srt"),
        (codec::subtitle::WEBVTT, "webvtt"),
        (codec::subtitle::ASS, "ass"),
        (codec::subtitle::SSA, "ssa"),
        (codec::subtitle::MOV_TEXT, "mov_text"),
    ];
    for (constant, value) in expected {
        assert_eq!(constant, value);
    }
}

#[test]
fn format_all_constants() {
    // Video
    assert_eq!(format::MP4, "mp4");
    assert_eq!(format::MKV, "mkv");
    assert_eq!(format::WEBM, "webm");
    assert_eq!(format::AVI, "avi");
    assert_eq!(format::MOV, "mov");
    assert_eq!(format::FLV, "flv");
    assert_eq!(format::TS, "ts");
    assert_eq!(format::M4V, "m4v");
    assert_eq!(format::WMV, "wmv");
    // Audio
    assert_eq!(format::MP3, "mp3");
    assert_eq!(format::WAV, "wav");
    assert_eq!(format::FLAC, "flac");
    assert_eq!(format::OGG, "ogg");
    assert_eq!(format::AAC, "aac");
    assert_eq!(format::M4A, "m4a");
    assert_eq!(format::WMA, "wma");
    assert_eq!(format::OPUS, "opus");
    // Image
    assert_eq!(format::PNG, "png");
    assert_eq!(format::JPEG, "jpeg");
    assert_eq!(format::WEBP, "webp");
    assert_eq!(format::GIF, "gif");
    assert_eq!(format::BMP, "bmp");
    assert_eq!(format::TIFF, "tiff");
    assert_eq!(format::SVG, "svg");
    assert_eq!(format::AVIF, "avif");
    assert_eq!(format::HEIF, "heif");
    // Subtitle
    assert_eq!(format::SRT, "srt");
    assert_eq!(format::VTT, "vtt");
    assert_eq!(format::ASS, "ass");
}

// ── 6. Registry deep tests ──────────────────────────────────────────────

#[test]
fn registry_format_from_extension() {
    let reg = Registry::default();
    for (ext, expected_id) in [
        ("mp4", "mp4"),
        ("wav", "wav"),
        ("png", "png"),
        ("webm", "webm"),
    ] {
        let info = reg.format_from_extension(ext);
        assert!(info.is_some(), "extension {ext} should be found");
        assert_eq!(info.unwrap().id, Format::new(expected_id));
    }
}

#[test]
fn registry_format_from_extension_case_insensitive() {
    let reg = Registry::default();
    let upper = reg.format_from_extension("MP4");
    let lower = reg.format_from_extension("mp4");
    assert!(upper.is_some());
    assert!(lower.is_some());
    assert_eq!(upper.unwrap().id, lower.unwrap().id);
}

#[test]
fn registry_format_from_mime() {
    let reg = Registry::default();
    for (mime, expected_id) in [
        ("video/mp4", "mp4"),
        ("audio/wav", "wav"),
        ("image/png", "png"),
    ] {
        let info = reg.format_from_mime(mime);
        assert!(info.is_some(), "mime {mime} should be found");
        assert_eq!(info.unwrap().id, Format::new(expected_id));
    }
}

#[test]
fn registry_codecs_by_kind_video() {
    let reg = Registry::default();
    let video_codecs = reg.codecs_by_kind(CodecKind::Video);
    assert!(!video_codecs.is_empty());
    assert!(
        video_codecs.iter().any(|c| c.id == Codec::new("h264")),
        "video codecs should include h264"
    );
}

#[test]
fn registry_codecs_by_kind_audio() {
    let reg = Registry::default();
    let audio_codecs = reg.codecs_by_kind(CodecKind::Audio);
    assert!(!audio_codecs.is_empty());
    assert!(
        audio_codecs.iter().any(|c| c.id == Codec::new("aac")),
        "audio codecs should include aac"
    );
}

#[test]
fn registry_formats_for_codec_h264() {
    let reg = Registry::default();
    let formats = reg.formats_for_codec(&Codec::new("h264"));
    let ids: Vec<&str> = formats.iter().map(|f| f.extension.as_str()).collect();
    for expected in ["mp4", "mkv", "avi", "mov", "ts"] {
        assert!(
            ids.contains(&expected),
            "h264 should be in {expected}, got: {ids:?}"
        );
    }
}

#[test]
fn registry_formats_for_codec_unknown() {
    let reg = Registry::default();
    let formats = reg.formats_for_codec(&Codec::new("totally_unknown_codec"));
    assert!(formats.is_empty());
}

#[test]
fn registry_default_codecs_mp4() {
    let reg = Registry::default();
    let (video, audio) = reg.default_codecs(&Format::new("mp4")).unwrap();
    assert_eq!(video, Codec::new("h264"));
    assert_eq!(audio, Codec::new("aac"));
}

#[test]
fn registry_default_codecs_webm() {
    let reg = Registry::default();
    let (video, audio) = reg.default_codecs(&Format::new("webm")).unwrap();
    assert_eq!(video, Codec::new("vp9"));
    assert_eq!(audio, Codec::new("opus"));
}

#[test]
fn registry_custom_format() {
    let mut reg = Registry::default();
    reg.register_format(registry::FormatInfo {
        id: Format::new("myformat"),
        extension: "myf".into(),
        mime_type: "application/x-myformat".into(),
        is_container: true,
        supported_media_types: vec![MediaType::Video],
        default_video_codec: None,
        default_audio_codec: None,
    });
    let info = reg.format_from_extension("myf");
    assert!(info.is_some());
    assert_eq!(info.unwrap().id, Format::new("myformat"));
}

#[test]
fn registry_format_info_mime_types() {
    let reg = Registry::default();
    let mp4 = reg.format_info(&Format::new("mp4")).unwrap();
    assert_eq!(mp4.mime_type, "video/mp4");

    let webm = reg.format_info(&Format::new("webm")).unwrap();
    assert_eq!(webm.mime_type, "video/webm");

    let mp3 = reg.format_info(&Format::new("mp3")).unwrap();
    assert_eq!(mp3.mime_type, "audio/mpeg");

    let png = reg.format_info(&Format::new("png")).unwrap();
    assert_eq!(png.mime_type, "image/png");
}

// ── 7. Filter tests ─────────────────────────────────────────────────────

#[test]
fn filter_convenience_denoise() {
    let f = filter::filters::denoise(3);
    assert_eq!(f.name, "denoise");
    assert_eq!(f.target, FilterTarget::Video);
    assert!(matches!(f.params.get("strength"), Some(ParamValue::Int(3))));
}

#[test]
fn filter_convenience_sharpen() {
    let f = filter::filters::sharpen(1.5);
    assert_eq!(f.name, "sharpen");
    assert_eq!(f.target, FilterTarget::Video);
    if let Some(ParamValue::Float(v)) = f.params.get("amount") {
        assert!((*v - 1.5).abs() < 0.01);
    } else {
        panic!("expected Float param 'amount'");
    }
}

#[test]
fn filter_convenience_blur() {
    let f = filter::filters::blur(2.0);
    assert_eq!(f.name, "blur");
    assert_eq!(f.target, FilterTarget::Video);
    if let Some(ParamValue::Float(v)) = f.params.get("radius") {
        assert!((*v - 2.0).abs() < 0.01);
    } else {
        panic!("expected Float param 'radius'");
    }
}

#[test]
fn filter_convenience_grayscale() {
    let f = filter::filters::grayscale();
    assert_eq!(f.name, "grayscale");
    assert_eq!(f.target, FilterTarget::Video);
    assert!(f.params.get("anything").is_none());
}

#[test]
fn filter_convenience_sepia() {
    let f = filter::filters::sepia();
    assert_eq!(f.name, "sepia");
    assert_eq!(f.target, FilterTarget::Video);
}

#[test]
fn filter_convenience_high_pass() {
    let f = filter::filters::high_pass(300);
    assert_eq!(f.name, "high_pass");
    assert_eq!(f.target, FilterTarget::Audio);
    assert!(matches!(
        f.params.get("frequency"),
        Some(ParamValue::Int(300))
    ));
}

#[test]
fn filter_convenience_low_pass() {
    let f = filter::filters::low_pass(8000);
    assert_eq!(f.name, "low_pass");
    assert_eq!(f.target, FilterTarget::Audio);
    assert!(matches!(
        f.params.get("frequency"),
        Some(ParamValue::Int(8000))
    ));
}

#[test]
fn filter_convenience_equalizer() {
    let f = filter::filters::equalizer(1000, 1.5, 3.0);
    assert_eq!(f.name, "equalizer");
    assert_eq!(f.target, FilterTarget::Audio);
    assert!(matches!(
        f.params.get("frequency"),
        Some(ParamValue::Int(1000))
    ));
    assert!(f.params.get("width").is_some());
    assert!(f.params.get("gain").is_some());
}

#[test]
fn filter_convenience_compressor() {
    let f = filter::filters::compressor(-20.0, 4.0);
    assert_eq!(f.name, "compressor");
    assert_eq!(f.target, FilterTarget::Audio);
    assert!(f.params.get("threshold").is_some());
    assert!(f.params.get("ratio").is_some());
}

#[test]
fn filter_convenience_noise_reduction() {
    let f = filter::filters::noise_reduction(0.5);
    assert_eq!(f.name, "noise_reduction");
    assert_eq!(f.target, FilterTarget::Audio);
    assert!(f.params.get("amount").is_some());
}

#[test]
fn filter_convenience_custom_video() {
    let f = filter::filters::custom_video("chromakey=0x00FF00:0.1:0.2");
    assert_eq!(f.name, "chromakey=0x00FF00:0.1:0.2");
    assert_eq!(f.target, FilterTarget::Video);
}

#[test]
fn filter_convenience_custom_audio() {
    let f = filter::filters::custom_audio("aecho=0.8:0.88:60:0.4");
    assert_eq!(f.name, "aecho=0.8:0.88:60:0.4");
    assert_eq!(f.target, FilterTarget::Audio);
}

#[test]
fn param_value_from_impls() {
    let int_val: ParamValue = 42i64.into();
    assert!(matches!(int_val, ParamValue::Int(42)));

    let float_val: ParamValue = 3.14f64.into();
    assert!(matches!(float_val, ParamValue::Float(v) if (v - 3.14).abs() < f64::EPSILON));

    let string_val: ParamValue = String::from("hello").into();
    assert!(matches!(string_val, ParamValue::Str(ref s) if s == "hello"));

    let str_val: ParamValue = "world".into();
    assert!(matches!(str_val, ParamValue::Str(ref s) if s == "world"));

    let bool_val: ParamValue = true.into();
    assert!(matches!(bool_val, ParamValue::Bool(true)));
}

// ── 8. Output config tests ──────────────────────────────────────────────

#[test]
fn output_config_builder() {
    let config = OutputConfig::new(Format::new("mp4"))
        .with_video(VideoSettings::new(Codec::new("h264")))
        .with_audio(AudioSettings::new(Codec::new("aac")))
        .with_strip_metadata()
        .with_param("movflags", "faststart");

    assert_eq!(config.format, Format::new("mp4"));
    assert!(config.video.is_some());
    assert!(config.audio.is_some());
    assert!(config.strip_metadata);
    assert_eq!(config.extra.get("movflags").unwrap(), "faststart");
}

#[test]
fn output_config_validate_compatible() {
    let reg = Registry::default();
    let config = OutputConfig::new(Format::new("mp4"))
        .with_video(VideoSettings::new(Codec::new("h264")))
        .with_audio(AudioSettings::new(Codec::new("aac")));
    assert!(config.validate(&reg).is_ok());
}

#[test]
fn output_config_validate_incompatible_video() {
    let reg = Registry::default();
    let config =
        OutputConfig::new(Format::new("webm")).with_video(VideoSettings::new(Codec::new("h264")));
    assert!(config.validate(&reg).is_err());
}

#[test]
fn output_config_validate_incompatible_audio() {
    let reg = Registry::default();
    let config =
        OutputConfig::new(Format::new("mp3")).with_video(VideoSettings::new(Codec::new("vp9")));
    assert!(config.validate(&reg).is_err());
}

#[test]
fn output_config_validate_no_codecs() {
    let reg = Registry::default();
    let config = OutputConfig::new(Format::new("mp4"));
    assert!(config.validate(&reg).is_ok());
}

#[test]
fn video_settings_builder() {
    let vs = VideoSettings::new(Codec::new("h264"))
        .with_resolution(Resolution::p1080())
        .with_frame_rate(FrameRate::fps_30())
        .with_quality(Quality::High)
        .with_bitrate(Bitrate::Constant(5_000_000))
        .with_speed(EncodingSpeed::Medium);

    assert_eq!(vs.codec, Codec::new("h264"));
    assert_eq!(vs.resolution, Some(Resolution::p1080()));
    assert_eq!(vs.frame_rate, Some(FrameRate::fps_30()));
    assert_eq!(vs.quality, Some(Quality::High));
    assert_eq!(vs.bitrate, Some(Bitrate::Constant(5_000_000)));
    assert_eq!(vs.speed, Some(EncodingSpeed::Medium));
}

#[test]
fn audio_settings_builder() {
    let a = AudioSettings::new(Codec::new("aac"))
        .with_sample_rate(audio::SampleRate::cd())
        .with_channels(audio::ChannelLayout::Stereo)
        .with_bitrate(Bitrate::Variable(128_000));

    assert_eq!(a.codec, Codec::new("aac"));
    assert_eq!(a.sample_rate, Some(audio::SampleRate::cd()));
    assert_eq!(a.channels, Some(audio::ChannelLayout::Stereo));
    assert_eq!(a.bitrate, Some(Bitrate::Variable(128_000)));
}

#[test]
fn quality_variants() {
    let variants = [
        Quality::Lossless,
        Quality::UltraHigh,
        Quality::High,
        Quality::Medium,
        Quality::Low,
        Quality::VeryLow,
        Quality::Custom(23),
    ];
    // All should be distinct from each other
    for (i, a) in variants.iter().enumerate() {
        for (j, b) in variants.iter().enumerate() {
            if i != j {
                assert_ne!(a, b);
            }
        }
    }
}

#[test]
fn bitrate_variants() {
    let c = Bitrate::Constant(5_000_000);
    let v = Bitrate::Variable(3_000_000);
    let cr = Bitrate::Constrained {
        target: 3_000_000,
        max: 5_000_000,
    };
    assert_ne!(c, v);
    assert_ne!(c, cr);
    assert_ne!(v, cr);
}

#[test]
fn encoding_speed_variants() {
    let speeds = [
        EncodingSpeed::UltraFast,
        EncodingSpeed::SuperFast,
        EncodingSpeed::VeryFast,
        EncodingSpeed::Fast,
        EncodingSpeed::Medium,
        EncodingSpeed::Slow,
        EncodingSpeed::VerySlow,
    ];
    for (i, a) in speeds.iter().enumerate() {
        for (j, b) in speeds.iter().enumerate() {
            if i != j {
                assert_ne!(a, b);
            }
        }
    }
}

// ── 9. Presets ──────────────────────────────────────────────────────────

#[test]
fn preset_mp4_h265() {
    let config = presets::mp4_h265();
    assert_eq!(config.format, Format::new("mp4"));
    let v = config.video.as_ref().unwrap();
    assert_eq!(v.codec, Codec::new("h265"));
    assert!(config.audio.is_some());
}

#[test]
fn preset_webm_av1() {
    let config = presets::webm_av1();
    assert_eq!(config.format, Format::new("webm"));
    let v = config.video.as_ref().unwrap();
    assert_eq!(v.codec, Codec::new("av1"));
    let a = config.audio.as_ref().unwrap();
    assert_eq!(a.codec, Codec::new("opus"));
}

#[test]
fn preset_mkv_h265() {
    let config = presets::mkv_h265();
    assert_eq!(config.format, Format::new("mkv"));
    let v = config.video.as_ref().unwrap();
    assert_eq!(v.codec, Codec::new("h265"));
}

#[test]
fn preset_flac() {
    let config = presets::flac();
    assert_eq!(config.format, Format::new("flac"));
    assert!(config.video.is_none());
    let a = config.audio.as_ref().unwrap();
    assert_eq!(a.codec, Codec::new("flac"));
}

#[test]
fn preset_ogg_opus() {
    let config = presets::ogg_opus();
    assert_eq!(config.format, Format::new("ogg"));
    assert!(config.video.is_none());
    let a = config.audio.as_ref().unwrap();
    assert_eq!(a.codec, Codec::new("opus"));
}

#[test]
fn preset_jpeg() {
    let config = presets::jpeg();
    assert_eq!(config.format, Format::new("jpeg"));
}

#[test]
fn preset_webp() {
    let config = presets::webp();
    assert_eq!(config.format, Format::new("webp"));
}

#[test]
fn preset_gif() {
    let config = presets::gif();
    assert_eq!(config.format, Format::new("gif"));
}

// ── 10. Pipeline tests ──────────────────────────────────────────────────

#[test]
fn pipeline_audio_operations() {
    let source = rskit_file::FileSource::from_path("/tmp/test.mp4");
    let pipe = pipeline::MediaPipeline::from(&source)
        .volume(0.5)
        .normalize_audio()
        .fade_in(Duration::from_secs(2))
        .fade_out(Duration::from_secs(3))
        .strip_audio()
        .strip_video();

    let ops = pipe.operations();
    assert_eq!(ops.len(), 6);
    assert!(matches!(ops[0], MediaOp::Volume(v) if (v - 0.5).abs() < f64::EPSILON));
    assert!(matches!(ops[1], MediaOp::NormalizeAudio));
    assert!(matches!(ops[2], MediaOp::FadeIn(_)));
    assert!(matches!(ops[3], MediaOp::FadeOut(_)));
    assert!(matches!(ops[4], MediaOp::StripAudio));
    assert!(matches!(ops[5], MediaOp::StripVideo));
}

#[test]
fn pipeline_reverse() {
    let source = rskit_file::FileSource::from_path("/tmp/test.mp4");
    let pipe = pipeline::MediaPipeline::from(&source).reverse();
    assert_eq!(pipe.operations().len(), 1);
    assert!(matches!(pipe.operations()[0], MediaOp::Reverse));
}

#[test]
fn pipeline_pad() {
    let source = rskit_file::FileSource::from_path("/tmp/test.mp4");
    let pipe = pipeline::MediaPipeline::from(&source).pad(1920, 1080, "black");
    assert_eq!(pipe.operations().len(), 1);
    assert!(matches!(pipe.operations()[0], MediaOp::Pad(_)));
}

#[test]
fn pipeline_select_tracks() {
    let source = rskit_file::FileSource::from_path("/tmp/test.mp4");
    let pipe = pipeline::MediaPipeline::from(&source)
        .select_tracks(vec![0, 2])
        .select_tracks_by_kind(vec![TrackKind::Video, TrackKind::Audio]);

    let ops = pipe.operations();
    assert_eq!(ops.len(), 2);
    assert!(matches!(&ops[0], MediaOp::SelectTracks(idx) if idx == &[0, 2]));
    assert!(matches!(&ops[1], MediaOp::SelectTracksByKind(kinds) if kinds.len() == 2));
}

#[test]
fn pipeline_estimated_duration_extract() {
    let source = rskit_file::FileSource::from_path("/tmp/test.mp4");
    let pipe = pipeline::MediaPipeline::from(&source).extract(TimeRange::from_seconds(10.0, 60.0));
    let est = pipe.estimated_duration(Duration::from_secs(120));
    assert_eq!(est, Duration::from_secs(50));
}

#[test]
fn pipeline_estimated_duration_speed() {
    let source = rskit_file::FileSource::from_path("/tmp/test.mp4");
    let pipe = pipeline::MediaPipeline::from(&source).speed(2.0);
    let est = pipe.estimated_duration(Duration::from_secs(120));
    assert_eq!(est, Duration::from_secs(60));
}

#[test]
fn pipeline_estimated_duration_no_ops() {
    let source = rskit_file::FileSource::from_path("/tmp/test.mp4");
    let pipe = pipeline::MediaPipeline::from(&source);
    let est = pipe.estimated_duration(Duration::from_secs(120));
    assert_eq!(est, Duration::from_secs(120));
}

#[test]
fn pipeline_concat_and_overlay() {
    let source = rskit_file::FileSource::from_path("/tmp/test.mp4");
    let other = rskit_file::FileSource::from_path("/tmp/other.mp4");
    let pipe = pipeline::MediaPipeline::from(&source)
        .concat(&other)
        .overlay(&other, ops::OverlayPosition::Center, 0.8);

    let ops_list = pipe.operations();
    assert_eq!(ops_list.len(), 2);
    assert!(matches!(ops_list[0], MediaOp::Concat(_)));
    assert!(matches!(ops_list[1], MediaOp::Overlay(_)));
}

// ── 11. Subtitle tests ──────────────────────────────────────────────────

#[test]
fn subtitle_track_new_empty() {
    let track = SubtitleTrack::new();
    assert!(track.entries.is_empty());
    assert!(track.language.is_none());
    assert!(track.default_style.is_none());
}

#[test]
fn subtitle_track_add_entries() {
    let track = SubtitleTrack::new()
        .add(TimeRange::from_seconds(0.0, 2.0), "First")
        .add(TimeRange::from_seconds(3.0, 5.0), "Second")
        .add(TimeRange::from_seconds(6.0, 8.0), "Third");
    assert_eq!(track.entries.len(), 3);
    assert_eq!(track.entries[0].text, "First");
    assert_eq!(track.entries[1].text, "Second");
    assert_eq!(track.entries[2].text, "Third");
}

#[test]
fn subtitle_track_with_language() {
    let track = SubtitleTrack::new().with_language("en-US");
    assert_eq!(track.language.as_deref(), Some("en-US"));
}

#[test]
fn subtitle_track_shift_positive() {
    let mut track = SubtitleTrack::new()
        .add(TimeRange::from_millis(1000, 2000), "Sub 1")
        .add(TimeRange::from_millis(3000, 4000), "Sub 2");
    track.shift(500);
    assert_eq!(track.entries[0].range.start, Timestamp::from_millis(1500));
    assert_eq!(track.entries[0].range.end, Timestamp::from_millis(2500));
    assert_eq!(track.entries[1].range.start, Timestamp::from_millis(3500));
    assert_eq!(track.entries[1].range.end, Timestamp::from_millis(4500));
}

#[test]
fn subtitle_track_shift_negative() {
    let mut track = SubtitleTrack::new().add(TimeRange::from_millis(2000, 4000), "Sub 1");
    track.shift(-1000);
    assert_eq!(track.entries[0].range.start, Timestamp::from_millis(1000));
    assert_eq!(track.entries[0].range.end, Timestamp::from_millis(3000));
}

#[test]
fn subtitle_track_in_range() {
    let track = SubtitleTrack::new()
        .add(TimeRange::from_seconds(1.0, 3.0), "A")
        .add(TimeRange::from_seconds(4.0, 6.0), "B")
        .add(TimeRange::from_seconds(7.0, 9.0), "C");

    let range = TimeRange::from_seconds(2.0, 5.0);
    let filtered = track.in_range(&range);
    assert_eq!(filtered.entries.len(), 2); // A and B overlap with [2, 5]
    assert_eq!(filtered.entries[0].text, "A");
    assert_eq!(filtered.entries[1].text, "B");
}

#[test]
fn srt_empty_input() {
    let track = SubtitleTrack::from_srt("").unwrap();
    assert!(track.entries.is_empty());
}

#[test]
fn vtt_empty_input() {
    let track = SubtitleTrack::from_vtt("").unwrap();
    assert!(track.entries.is_empty());
}

#[test]
fn vtt_roundtrip() {
    let original = SubtitleTrack::new()
        .add(TimeRange::from_millis(1000, 4000), "Hello, world!")
        .add(TimeRange::from_millis(5000, 8000), "Second entry");

    let vtt_str = original.to_vtt();
    let parsed = SubtitleTrack::from_vtt(&vtt_str).expect("should re-parse VTT");
    assert_eq!(parsed.entries.len(), original.entries.len());
    for (a, b) in original.entries.iter().zip(parsed.entries.iter()) {
        assert_eq!(a.range.start, b.range.start);
        assert_eq!(a.range.end, b.range.end);
        assert_eq!(a.text, b.text);
    }
}

#[test]
fn subtitle_style_defaults() {
    let style = SubtitleStyle {
        font_family: None,
        font_size: None,
        color: None,
        background: None,
        bold: false,
        italic: false,
        position: SubtitlePosition::default(),
    };
    assert!(!style.bold);
    assert!(!style.italic);
    assert!(matches!(style.position, SubtitlePosition::Bottom));
}

#[test]
fn subtitle_position_variants() {
    let _bottom = SubtitlePosition::Bottom;
    let _top = SubtitlePosition::Top;
    let _center = SubtitlePosition::Center;
    let custom = SubtitlePosition::Custom { x: 100, y: 200 };
    if let SubtitlePosition::Custom { x, y } = custom {
        assert_eq!(x, 100);
        assert_eq!(y, 200);
    } else {
        panic!("expected Custom variant");
    }
}

// ── 12. Audio module tests ──────────────────────────────────────────────

#[test]
fn sample_rate_presets() {
    assert_eq!(audio::SampleRate::cd().0, 44100);
    assert_eq!(audio::SampleRate::dvd().0, 48000);
    assert_eq!(audio::SampleRate::hd().0, 96000);
}

#[test]
fn sample_rate_hz() {
    let sr = audio::SampleRate::hz(22050);
    assert_eq!(sr.0, 22050);
}

#[test]
fn channel_layout_surround71() {
    assert_eq!(audio::ChannelLayout::Surround71.channel_count(), 8);
}

#[test]
fn channel_layout_custom() {
    let cl = audio::ChannelLayout::Custom(4);
    assert_eq!(cl.channel_count(), 4);
}

#[test]
fn sample_rate_serde_roundtrip() {
    for sr in [
        audio::SampleRate::cd(),
        audio::SampleRate::dvd(),
        audio::SampleRate::hd(),
        audio::SampleRate::hz(22050),
    ] {
        let json = serde_json::to_string(&sr).unwrap();
        let sr2: audio::SampleRate = serde_json::from_str(&json).unwrap();
        assert_eq!(sr, sr2);
    }
}

#[test]
fn channel_layout_serde_roundtrip() {
    for cl in [
        audio::ChannelLayout::Mono,
        audio::ChannelLayout::Stereo,
        audio::ChannelLayout::Surround51,
        audio::ChannelLayout::Surround71,
        audio::ChannelLayout::Custom(4),
    ] {
        let json = serde_json::to_string(&cl).unwrap();
        let cl2: audio::ChannelLayout = serde_json::from_str(&json).unwrap();
        assert_eq!(cl, cl2);
    }
}

// ── 13. Track types ─────────────────────────────────────────────────────

#[test]
fn track_kind_all_variants() {
    let variants = [
        TrackKind::Video,
        TrackKind::Audio,
        TrackKind::Subtitle,
        TrackKind::Data,
        TrackKind::Attachment,
    ];
    // Each variant should be unique
    for (i, a) in variants.iter().enumerate() {
        for (j, b) in variants.iter().enumerate() {
            if i != j {
                assert_ne!(a, b);
            }
        }
    }
}

#[test]
fn media_type_serde_roundtrip() {
    for mt in [MediaType::Video, MediaType::Audio, MediaType::Image] {
        let json = serde_json::to_string(&mt).unwrap();
        let mt2: MediaType = serde_json::from_str(&json).unwrap();
        assert_eq!(mt, mt2);
    }
}

#[test]
fn track_kind_serde_roundtrip() {
    for tk in [
        TrackKind::Video,
        TrackKind::Audio,
        TrackKind::Subtitle,
        TrackKind::Data,
        TrackKind::Attachment,
    ] {
        let json = serde_json::to_string(&tk).unwrap();
        let tk2: TrackKind = serde_json::from_str(&json).unwrap();
        assert_eq!(tk, tk2);
    }
}

// ── 14. Segment tests ───────────────────────────────────────────────────

#[test]
fn segment_with_meta() {
    let seg = time::Segment::new(TimeRange::from_millis(0, 5000))
        .with_meta("scene", "outdoor")
        .with_meta("score", 42);
    assert_eq!(
        seg.metadata.get("scene").and_then(|v| v.as_str()),
        Some("outdoor")
    );
    assert_eq!(seg.metadata.get("score").and_then(|v| v.as_i64()), Some(42));
}

#[test]
fn segment_serde_roundtrip() {
    let seg = time::Segment::new(TimeRange::from_millis(1000, 3000))
        .with_label("chorus")
        .with_confidence(0.88);
    let json = serde_json::to_string(&seg).unwrap();
    let seg2: time::Segment = serde_json::from_str(&json).unwrap();
    assert_eq!(seg.range, seg2.range);
    assert_eq!(seg.label, seg2.label);
    assert!((seg.confidence.unwrap() - seg2.confidence.unwrap()).abs() < f32::EPSILON);
}

#[test]
fn segment_all_fields() {
    let seg = time::Segment::new(TimeRange::from_millis(500, 1500))
        .with_label("verse")
        .with_confidence(0.75)
        .with_meta("key", "Am")
        .with_meta("bpm", 120);

    assert_eq!(seg.range.start, Timestamp::from_millis(500));
    assert_eq!(seg.range.end, Timestamp::from_millis(1500));
    assert_eq!(seg.label.as_deref(), Some("verse"));
    assert!((seg.confidence.unwrap() - 0.75).abs() < f32::EPSILON);
    assert_eq!(seg.metadata.len(), 2);
}
