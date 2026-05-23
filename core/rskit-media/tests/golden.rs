use rskit_media::{
    MediaPipeline, Registry,
    codec::{self, Codec, CodecKind},
    filter::filters,
    format::{self, Format},
    ops::{
        FilterConfig, FilterPreset, FlipDirection, ImageFormat, InterpolateConfig,
        InterpolateModel, MediaOp, OverlayConfig, OverlayPosition, OverlayType, Position,
        ResizeMode, SceneDetectConfig, Size, SubtitleConfig, SubtitleFormat, SubtitleSource,
        TextOverlay, ThumbnailConfig, UpscaleConfig, UpscaleModel,
    },
    presets,
    spatial::Resolution,
    subtitle::SubtitleTrack,
    time::{Segment, TimeRange},
};
use rskit_storage::{FileSink, FileSource};
use std::time::Duration;

struct DeterministicExecutor;

#[async_trait::async_trait]
impl rskit_media::executor::MediaExecutor for DeterministicExecutor {
    async fn execute(
        &self,
        source: &FileSource,
        ops: &[MediaOp],
        sink: Option<&FileSink>,
    ) -> rskit_errors::AppResult<FileSource> {
        let bytes = source.read_all().await?;
        let output = format!(
            "fixture_hash={:016x}\nsink={sink:?}\nops={ops:#?}\noutput_hash={:016x}\n",
            stable_hash(&bytes),
            stable_hash(format!("{ops:#?}|{sink:?}|{:?}", bytes.as_ref()).as_bytes()),
        );
        Ok(FileSource::from_bytes(output.into_bytes()))
    }

    async fn execute_with_progress(
        &self,
        source: &FileSource,
        ops: &[MediaOp],
        sink: Option<&FileSink>,
        _on_progress: Box<dyn Fn(rskit_media::pipeline::Progress) + Send + Sync>,
    ) -> rskit_errors::AppResult<FileSource> {
        self.execute(source, ops, sink).await
    }

    fn supports(&self, _op: &MediaOp) -> bool {
        true
    }

    fn preview(
        &self,
        _source: &FileSource,
        ops: &[MediaOp],
    ) -> rskit_errors::AppResult<Vec<String>> {
        Ok(ops.iter().map(|op| format!("{op:?}")).collect())
    }
}

fn stable_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

async fn snapshot_executed_pipeline(name: &str, pipeline: MediaPipeline) {
    let output = pipeline
        .execute(&DeterministicExecutor)
        .await
        .expect("deterministic executor should run");
    let bytes = output.read_all().await.expect("output should be readable");
    let text = String::from_utf8(bytes.to_vec()).expect("output is utf8");
    insta::assert_snapshot!(name, text);
}

// ── Registry Codec Lookups ──────────────────────────────────────────

#[test]
fn codec_info_h264() {
    let reg = Registry::default();
    let info = reg.codec_info(&Codec::new(codec::video::H264)).unwrap();
    insta::assert_debug_snapshot!("codec_info_h264", info);
}

#[test]
fn codec_info_h265() {
    let reg = Registry::default();
    let info = reg.codec_info(&Codec::new(codec::video::H265)).unwrap();
    insta::assert_debug_snapshot!("codec_info_h265", info);
}

#[test]
fn codec_info_vp9() {
    let reg = Registry::default();
    let info = reg.codec_info(&Codec::new(codec::video::VP9)).unwrap();
    insta::assert_debug_snapshot!("codec_info_vp9", info);
}

#[test]
fn codec_info_aac() {
    let reg = Registry::default();
    let info = reg.codec_info(&Codec::new(codec::audio::AAC)).unwrap();
    insta::assert_debug_snapshot!("codec_info_aac", info);
}

#[test]
fn codec_info_opus() {
    let reg = Registry::default();
    let info = reg.codec_info(&Codec::new(codec::audio::OPUS)).unwrap();
    insta::assert_debug_snapshot!("codec_info_opus", info);
}

#[test]
fn codec_info_pcm() {
    let reg = Registry::default();
    let info = reg.codec_info(&Codec::new(codec::audio::PCM)).unwrap();
    insta::assert_debug_snapshot!("codec_info_pcm", info);
}

// JPEG and PNG are only registered as formats, not codecs
#[test]
fn codec_info_jpeg_not_registered() {
    let reg = Registry::default();
    let info = reg.codec_info(&Codec::new(codec::image::JPEG));
    insta::assert_debug_snapshot!("codec_info_jpeg_not_registered", info);
}

#[test]
fn codec_info_png_not_registered() {
    let reg = Registry::default();
    let info = reg.codec_info(&Codec::new(codec::image::PNG));
    insta::assert_debug_snapshot!("codec_info_png_not_registered", info);
}

// ── Registry Format Lookups ─────────────────────────────────────────

#[test]
fn format_info_jpeg() {
    let reg = Registry::default();
    let info = reg.format_info(&Format::new(format::JPEG)).unwrap();
    insta::assert_debug_snapshot!("format_info_jpeg", info);
}

#[test]
fn format_info_png() {
    let reg = Registry::default();
    let info = reg.format_info(&Format::new(format::PNG)).unwrap();
    insta::assert_debug_snapshot!("format_info_png", info);
}

#[test]
fn format_info_mp4() {
    let reg = Registry::default();
    let info = reg.format_info(&Format::new(format::MP4)).unwrap();
    insta::assert_debug_snapshot!("format_info_mp4", info);
}

#[test]
fn format_info_webm() {
    let reg = Registry::default();
    let info = reg.format_info(&Format::new(format::WEBM)).unwrap();
    insta::assert_debug_snapshot!("format_info_webm", info);
}

#[test]
fn format_info_wav() {
    let reg = Registry::default();
    let info = reg.format_info(&Format::new(format::WAV)).unwrap();
    insta::assert_debug_snapshot!("format_info_wav", info);
}

#[test]
fn format_info_mp3() {
    let reg = Registry::default();
    let info = reg.format_info(&Format::new(format::MP3)).unwrap();
    insta::assert_debug_snapshot!("format_info_mp3", info);
}

// ── Codec & Format Construction ─────────────────────────────────────

#[test]
fn codec_construction() {
    insta::assert_json_snapshot!("codec_h264", Codec::new(codec::video::H264));
    insta::assert_json_snapshot!("codec_h265", Codec::new(codec::video::H265));
    insta::assert_json_snapshot!("codec_vp9", Codec::new(codec::video::VP9));
    insta::assert_json_snapshot!("codec_aac", Codec::new(codec::audio::AAC));
    insta::assert_json_snapshot!("codec_opus", Codec::new(codec::audio::OPUS));
    insta::assert_json_snapshot!("codec_pcm", Codec::new(codec::audio::PCM));
}

#[test]
fn format_construction() {
    insta::assert_json_snapshot!("format_mp4", Format::new(format::MP4));
    insta::assert_json_snapshot!("format_webm", Format::new(format::WEBM));
    insta::assert_json_snapshot!("format_wav", Format::new(format::WAV));
    insta::assert_json_snapshot!("format_mp3", Format::new(format::MP3));
    insta::assert_json_snapshot!("format_jpeg", Format::new(format::JPEG));
    insta::assert_json_snapshot!("format_png", Format::new(format::PNG));
}

// ── Registry Compatibility ──────────────────────────────────────────

#[test]
fn formats_for_codec_h264() {
    let reg = Registry::default();
    let formats = reg.formats_for_codec(&Codec::new(codec::video::H264));
    let mut ids: Vec<&str> = formats.iter().map(|f| f.id.id()).collect();
    ids.sort();
    insta::assert_debug_snapshot!("formats_for_codec_h264", ids);
}

#[test]
fn formats_for_codec_aac() {
    let reg = Registry::default();
    let formats = reg.formats_for_codec(&Codec::new(codec::audio::AAC));
    let mut ids: Vec<&str> = formats.iter().map(|f| f.id.id()).collect();
    ids.sort();
    insta::assert_debug_snapshot!("formats_for_codec_aac", ids);
}

#[test]
fn formats_for_codec_vp9() {
    let reg = Registry::default();
    let formats = reg.formats_for_codec(&Codec::new(codec::video::VP9));
    let mut ids: Vec<&str> = formats.iter().map(|f| f.id.id()).collect();
    ids.sort();
    insta::assert_debug_snapshot!("formats_for_codec_vp9", ids);
}

#[test]
fn formats_for_codec_opus() {
    let reg = Registry::default();
    let formats = reg.formats_for_codec(&Codec::new(codec::audio::OPUS));
    let mut ids: Vec<&str> = formats.iter().map(|f| f.id.id()).collect();
    ids.sort();
    insta::assert_debug_snapshot!("formats_for_codec_opus", ids);
}

/// Collects all codecs compatible with a given format by checking each
/// registered codec via `is_compatible`. The registry does not expose a
/// direct `codecs_for_format` method, so we build it from primitives.
fn collect_codecs_for_format(reg: &Registry, format: &Format) -> Vec<String> {
    let mut result = Vec::new();
    for kind in [CodecKind::Video, CodecKind::Audio] {
        for info in reg.codecs_by_kind(kind) {
            if reg.is_compatible(&info.id, format) {
                result.push(info.id.to_string());
            }
        }
    }
    result.sort();
    result
}

#[test]
fn codecs_for_format_mp4() {
    let reg = Registry::default();
    let codecs = collect_codecs_for_format(&reg, &Format::new(format::MP4));
    insta::assert_debug_snapshot!("codecs_for_format_mp4", codecs);
}

#[test]
fn codecs_for_format_webm() {
    let reg = Registry::default();
    let codecs = collect_codecs_for_format(&reg, &Format::new(format::WEBM));
    insta::assert_debug_snapshot!("codecs_for_format_webm", codecs);
}

#[test]
fn codecs_for_format_wav() {
    let reg = Registry::default();
    let codecs = collect_codecs_for_format(&reg, &Format::new(format::WAV));
    insta::assert_debug_snapshot!("codecs_for_format_wav", codecs);
}

#[test]
fn codecs_for_format_mkv() {
    let reg = Registry::default();
    let codecs = collect_codecs_for_format(&reg, &Format::new(format::MKV));
    insta::assert_debug_snapshot!("codecs_for_format_mkv", codecs);
}

#[tokio::test]
async fn media_pipeline_temporal_spatial_audio_golden() {
    let source = FileSource::from_bytes(b"rskit golden video fixture v1".to_vec());
    let pipeline = MediaPipeline::from(&source)
        .extract(TimeRange::from_seconds(1.0, 3.0))
        .extract_many(vec![
            Segment::new(TimeRange::from_seconds(4.0, 6.0)).with_label("clip"),
        ])
        .resize(Resolution::new(1920, 1080), ResizeMode::Fit)
        .crop(rskit_media::ops::CropRegion::new(10, 20, 640, 360))
        .rotate(rskit_media::ops::Rotation::Degrees90)
        .flip(FlipDirection::Horizontal)
        .pad(1920, 1080, "black")
        .speed(1.25)
        .reverse()
        .volume(0.8)
        .normalize_audio()
        .fade_in(Duration::from_secs(2))
        .fade_out(Duration::from_secs(3))
        .strip_audio()
        .strip_video()
        .filter(filters::denoise(3))
        .transcode(presets::mp4_h264());

    snapshot_executed_pipeline("media_pipeline_temporal_spatial_audio", pipeline).await;
}

#[tokio::test]
async fn media_pipeline_composition_advanced_golden() {
    let source = FileSource::from_bytes(b"rskit golden video fixture v1".to_vec());
    let overlay_source = FileSource::from_bytes(b"rskit golden overlay fixture v1".to_vec());
    let audio_source = FileSource::from_bytes(b"rskit golden audio fixture v1".to_vec());
    let subtitle_track = SubtitleTrack::new().add(TimeRange::from_seconds(0.0, 1.5), "hello");

    let pipeline = MediaPipeline::from(&source)
        .overlay(&overlay_source, OverlayPosition::TopLeft(10, 20), 0.75)
        .concat_with_transition(
            &FileSource::from_path("/fixtures/second.mp4"),
            rskit_media::ops::Transition::Cut,
        )
        .replace_audio(&audio_source)
        .mix_audio(&audio_source, 0.5)
        .burn_subtitles(subtitle_track)
        .apply_filter(FilterConfig {
            preset: FilterPreset::Warm,
            intensity: 0.4,
            custom_params: None,
        })
        .add_overlay(OverlayConfig {
            overlay_type: OverlayType::Text(TextOverlay {
                text: "sample".to_string(),
                font_family: Some("Inter".to_string()),
                font_size: Some(24),
                color: Some("#ffffff".to_string()),
            }),
            position: Position { x: 0.5, y: 0.1 },
            size: Some(Size {
                width: 0.3,
                height: 0.1,
            }),
            opacity: 0.9,
            time_range: Some(TimeRange::from_seconds(0.0, 2.0)),
        })
        .generate_thumbnail(ThumbnailConfig {
            timestamp: 1.0,
            width: Some(320),
            height: None,
            format: ImageFormat::Jpeg,
            quality: Some(85),
        })
        .detect_scenes(SceneDetectConfig::default())
        .add_subtitles(SubtitleConfig {
            source: SubtitleSource::Inline("1\n00:00:00,000 --> 00:00:01,000\nhello".to_string()),
            format: SubtitleFormat::Srt,
            style: None,
        })
        .upscale(UpscaleConfig {
            model: UpscaleModel::RealEsrganX4Plus,
            scale: 4,
            denoise_strength: Some(0.2),
        })
        .interpolate(InterpolateConfig {
            model: InterpolateModel::Rife,
            multiplier: 2,
        })
        .select_tracks(vec![0, 1])
        .select_tracks_by_kind(vec![
            rskit_media::TrackKind::Video,
            rskit_media::TrackKind::Audio,
        ]);

    snapshot_executed_pipeline("media_pipeline_composition_advanced", pipeline).await;
}
