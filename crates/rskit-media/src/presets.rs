//! Preset output configurations for common formats.

use crate::{
    codec::{self, Codec},
    format::{self, Format},
    output::{AudioSettings, OutputConfig, Quality, VideoSettings},
};

// ── Video presets ────────────────────────────────────────────────────

/// MP4 with H.264 video and AAC audio.
pub fn mp4_h264() -> OutputConfig {
    OutputConfig::new(Format::new(format::MP4))
        .with_video(
            VideoSettings::new(Codec::new(codec::video::H264)).with_quality(Quality::Medium),
        )
        .with_audio(AudioSettings::new(Codec::new(codec::audio::AAC)))
}

/// MP4 with H.265 video and AAC audio.
pub fn mp4_h265() -> OutputConfig {
    OutputConfig::new(Format::new(format::MP4))
        .with_video(
            VideoSettings::new(Codec::new(codec::video::H265)).with_quality(Quality::Medium),
        )
        .with_audio(AudioSettings::new(Codec::new(codec::audio::AAC)))
}

/// WebM with VP9 video and Opus audio.
pub fn webm_vp9() -> OutputConfig {
    OutputConfig::new(Format::new(format::WEBM))
        .with_video(VideoSettings::new(Codec::new(codec::video::VP9)))
        .with_audio(AudioSettings::new(Codec::new(codec::audio::OPUS)))
}

/// WebM with AV1 video and Opus audio.
pub fn webm_av1() -> OutputConfig {
    OutputConfig::new(Format::new(format::WEBM))
        .with_video(VideoSettings::new(Codec::new(codec::video::AV1)))
        .with_audio(AudioSettings::new(Codec::new(codec::audio::OPUS)))
}

/// MKV with H.265 video and AAC audio.
pub fn mkv_h265() -> OutputConfig {
    OutputConfig::new(Format::new(format::MKV))
        .with_video(VideoSettings::new(Codec::new(codec::video::H265)))
        .with_audio(AudioSettings::new(Codec::new(codec::audio::AAC)))
}

// ── Audio-only presets ───────────────────────────────────────────────

/// MP3 audio.
pub fn mp3() -> OutputConfig {
    OutputConfig::new(Format::new(format::MP3))
        .with_audio(AudioSettings::new(Codec::new(codec::audio::MP3)))
}

/// WAV (uncompressed PCM).
pub fn wav() -> OutputConfig {
    OutputConfig::new(Format::new(format::WAV))
        .with_audio(AudioSettings::new(Codec::new(codec::audio::PCM)))
}

/// FLAC (lossless compressed).
pub fn flac() -> OutputConfig {
    OutputConfig::new(Format::new(format::FLAC))
        .with_audio(AudioSettings::new(Codec::new(codec::audio::FLAC)))
}

/// OGG with Opus audio.
pub fn ogg_opus() -> OutputConfig {
    OutputConfig::new(Format::new(format::OGG))
        .with_audio(AudioSettings::new(Codec::new(codec::audio::OPUS)))
}

// ── Image presets ────────────────────────────────────────────────────

/// PNG image output.
pub fn png() -> OutputConfig {
    OutputConfig::new(Format::new(format::PNG))
}

/// JPEG image output.
pub fn jpeg() -> OutputConfig {
    OutputConfig::new(Format::new(format::JPEG))
}

/// WebP image output.
pub fn webp() -> OutputConfig {
    OutputConfig::new(Format::new(format::WEBP))
}

/// GIF image output.
pub fn gif() -> OutputConfig {
    OutputConfig::new(Format::new(format::GIF))
}
