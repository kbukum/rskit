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
    assert!(reg.is_compatible(&h264, &mp4), "h264 should be compatible with mp4");
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
    assert!(matches!(params.get("enabled"), Some(ParamValue::Bool(true))));
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
    let pipe = pipeline::MediaPipeline::from(&source)
        .extract(TimeRange::from_seconds(1.0, 5.0));
    assert_eq!(pipe.operations().len(), 1);
    assert!(matches!(pipe.operations()[0], MediaOp::Extract(_)));
}

#[test]
fn pipeline_transcode() {
    let source = rskit_file::FileSource::from_path("/tmp/test.mp4");
    let pipe = pipeline::MediaPipeline::from(&source)
        .transcode(presets::webm_vp9());
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
