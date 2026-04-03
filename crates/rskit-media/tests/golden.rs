use rskit_media::{
    Registry,
    codec::{self, Codec, CodecKind},
    format::{self, Format},
};

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
