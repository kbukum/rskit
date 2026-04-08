//! Codec identifiers and well-known constants.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// An open codec identifier.
///
/// Use well-known constants from the submodules ([`video`], [`audio`],
/// [`image`], [`subtitle`]) or create custom identifiers.
///
/// # Examples
///
/// ```rust
/// use rskit_media::codec::{self, Codec, CodecKind};
///
/// let h264 = Codec::new(codec::video::H264);
/// let custom = Codec::new("my_proprietary_codec");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Codec(Arc<str>);

impl Codec {
    /// Create a new codec identifier.
    pub fn new(id: impl Into<Arc<str>>) -> Self {
        Self(id.into())
    }

    /// The codec identifier string.
    pub fn id(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Codec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Which domain a codec belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CodecKind {
    /// Video codec.
    Video,
    /// Audio codec.
    Audio,
    /// Image codec.
    Image,
    /// Subtitle codec.
    Subtitle,
    /// Unknown or unrecognized codec kind.
    Unknown,
}

/// Well-known video codecs.
pub mod video {
    /// H.264 / AVC.
    pub const H264: &str = "h264";
    /// H.265 / HEVC.
    pub const H265: &str = "h265";
    /// VP8.
    pub const VP8: &str = "vp8";
    /// VP9.
    pub const VP9: &str = "vp9";
    /// AV1.
    pub const AV1: &str = "av1";
    /// Apple ProRes.
    pub const PRORES: &str = "prores";
    /// MPEG-2.
    pub const MPEG2: &str = "mpeg2";
    /// MPEG-4 Part 2.
    pub const MPEG4: &str = "mpeg4";
    /// Theora.
    pub const THEORA: &str = "theora";
    /// Windows Media Video 3.
    pub const WMV3: &str = "wmv3";
}

/// Well-known audio codecs.
pub mod audio {
    /// Advanced Audio Coding.
    pub const AAC: &str = "aac";
    /// Opus.
    pub const OPUS: &str = "opus";
    /// MPEG Audio Layer III.
    pub const MP3: &str = "mp3";
    /// Free Lossless Audio Codec.
    pub const FLAC: &str = "flac";
    /// Vorbis.
    pub const VORBIS: &str = "vorbis";
    /// Pulse-code modulation (uncompressed).
    pub const PCM: &str = "pcm";
    /// Dolby Digital.
    pub const AC3: &str = "ac3";
    /// Dolby Digital Plus.
    pub const EAC3: &str = "eac3";
    /// Windows Media Audio.
    pub const WMA: &str = "wma";
    /// Apple Lossless Audio Codec.
    pub const ALAC: &str = "alac";
}

/// Well-known image codecs.
pub mod image {
    /// PNG.
    pub const PNG: &str = "png";
    /// JPEG.
    pub const JPEG: &str = "jpeg";
    /// WebP.
    pub const WEBP: &str = "webp";
    /// GIF.
    pub const GIF: &str = "gif";
    /// BMP.
    pub const BMP: &str = "bmp";
    /// TIFF.
    pub const TIFF: &str = "tiff";
    /// AVIF.
    pub const AVIF: &str = "avif";
    /// HEIF.
    pub const HEIF: &str = "heif";
}

/// Well-known subtitle codecs.
pub mod subtitle {
    /// SubRip.
    pub const SRT: &str = "srt";
    /// WebVTT.
    pub const WEBVTT: &str = "webvtt";
    /// Advanced SubStation Alpha.
    pub const ASS: &str = "ass";
    /// SubStation Alpha.
    pub const SSA: &str = "ssa";
    /// MOV text (mp4 subtitles).
    pub const MOV_TEXT: &str = "mov_text";
}

/// Codec profile for quality/compatibility targeting.
///
/// Profiles define feature subsets of a codec. Higher profiles enable more
/// features but require more processing power and may reduce compatibility.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CodecProfile {
    // ── H.264 / AVC ─────────────────────────────────────────────────
    /// H.264 Baseline (mobile, video conferencing).
    H264Baseline,
    /// H.264 Main (broadcast, streaming).
    H264Main,
    /// H.264 High (Blu-ray, high-quality streaming).
    H264High,
    /// H.264 High 10-bit.
    H264High10,
    /// H.264 High 4:2:2.
    H264High422,
    /// H.264 High 4:4:4 Predictive.
    H264High444,

    // ── H.265 / HEVC ────────────────────────────────────────────────
    /// HEVC Main (8-bit SDR).
    HevcMain,
    /// HEVC Main 10 (10-bit, HDR).
    HevcMain10,
    /// HEVC Main 12.
    HevcMain12,
    /// HEVC Main Still Picture.
    HevcMainStillPicture,

    // ── VP9 ─────────────────────────────────────────────────────────
    /// VP9 Profile 0 (4:2:0 8-bit).
    Vp9Profile0,
    /// VP9 Profile 1 (4:2:2/4:4:4 8-bit).
    Vp9Profile1,
    /// VP9 Profile 2 (4:2:0 10/12-bit).
    Vp9Profile2,
    /// VP9 Profile 3 (4:2:2/4:4:4 10/12-bit).
    Vp9Profile3,

    // ── AV1 ─────────────────────────────────────────────────────────
    /// AV1 Main (4:2:0 8/10-bit).
    Av1Main,
    /// AV1 High (4:4:4 8/10-bit).
    Av1High,
    /// AV1 Professional (4:2:2, 12-bit).
    Av1Professional,

    // ── AAC ─────────────────────────────────────────────────────────
    /// AAC Low Complexity (most common).
    AacLc,
    /// AAC High Efficiency (HE-AAC v1).
    AacHe,
    /// AAC High Efficiency v2 (HE-AAC v2).
    AacHeV2,

    // ── ProRes ──────────────────────────────────────────────────────
    /// ProRes 422 Proxy.
    ProResProxy,
    /// ProRes 422 LT.
    ProResLt,
    /// ProRes 422.
    ProRes422,
    /// ProRes 422 HQ.
    ProResHq,
    /// ProRes 4444.
    ProRes4444,

    /// Custom/other profile string.
    Other(String),
}

impl CodecProfile {
    /// Convert to FFmpeg `-profile:v` or `-profile:a` argument value.
    pub fn as_ffmpeg_arg(&self) -> &str {
        match self {
            Self::H264Baseline => "baseline",
            Self::H264Main => "main",
            Self::H264High => "high",
            Self::H264High10 => "high10",
            Self::H264High422 => "high422",
            Self::H264High444 => "high444p",
            Self::HevcMain => "main",
            Self::HevcMain10 => "main10",
            Self::HevcMain12 => "main12",
            Self::HevcMainStillPicture => "mainstillpicture",
            Self::Vp9Profile0 => "0",
            Self::Vp9Profile1 => "1",
            Self::Vp9Profile2 => "2",
            Self::Vp9Profile3 => "3",
            Self::Av1Main => "0",
            Self::Av1High => "1",
            Self::Av1Professional => "2",
            Self::AacLc => "aac_low",
            Self::AacHe => "aac_he",
            Self::AacHeV2 => "aac_he_v2",
            Self::ProResProxy => "0",
            Self::ProResLt => "1",
            Self::ProRes422 => "2",
            Self::ProResHq => "3",
            Self::ProRes4444 => "4",
            Self::Other(s) => s.as_str(),
        }
    }

    /// Parse from ffprobe's `profile` field.
    ///
    /// ffprobe returns human-readable strings like "High", "Main", "Baseline",
    /// "High 10", "High 4:4:4 Predictive", etc.
    pub fn from_ffprobe(s: &str) -> Option<Self> {
        // Normalize for matching
        let lower = s.to_lowercase();
        let trimmed = lower.trim();

        match trimmed {
            // H.264
            "constrained baseline" | "baseline" => Some(Self::H264Baseline),
            "main" => Some(Self::H264Main),
            "high" => Some(Self::H264High),
            "high 10" | "high10" => Some(Self::H264High10),
            "high 4:2:2" | "high422" => Some(Self::H264High422),
            "high 4:4:4 predictive" | "high444" | "high 4:4:4" => Some(Self::H264High444),
            // H.265 / HEVC
            "main still picture" => Some(Self::HevcMainStillPicture),
            "main 10" | "main10" => Some(Self::HevcMain10),
            "main 12" | "main12" => Some(Self::HevcMain12),
            // VP9
            "profile 0" => Some(Self::Vp9Profile0),
            "profile 1" => Some(Self::Vp9Profile1),
            "profile 2" => Some(Self::Vp9Profile2),
            "profile 3" => Some(Self::Vp9Profile3),
            // AAC
            "lc" | "aac-lc" => Some(Self::AacLc),
            "he-aac" | "he-aacv1" => Some(Self::AacHe),
            "he-aacv2" => Some(Self::AacHeV2),
            // ProRes
            "apco" | "proxy" => Some(Self::ProResProxy),
            "apcs" | "lt" => Some(Self::ProResLt),
            "apcn" | "standard" | "422" => Some(Self::ProRes422),
            "apch" | "hq" => Some(Self::ProResHq),
            "ap4h" | "4444" => Some(Self::ProRes4444),
            _ => {
                if trimmed.is_empty() || trimmed == "unknown" {
                    None
                } else {
                    Some(Self::Other(s.into()))
                }
            }
        }
    }
}

/// Codec level constraining resolution, bitrate, and framerate.
///
/// Levels are codec-specific. For H.264/H.265 they map directly to the
/// standard levels (e.g., 3.0, 4.1, 5.1).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CodecLevel(Arc<str>);

impl CodecLevel {
    /// Create a new codec level.
    pub fn new(level: impl Into<Arc<str>>) -> Self {
        Self(level.into())
    }

    /// The level string.
    pub fn id(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CodecLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Well-known H.264 levels.
pub mod levels {
    use super::CodecLevel;

    /// H.264 Level 3.0 (SD video).
    pub fn h264_3_0() -> CodecLevel {
        CodecLevel::new("3.0")
    }
    /// H.264 Level 3.1 (720p30).
    pub fn h264_3_1() -> CodecLevel {
        CodecLevel::new("3.1")
    }
    /// H.264 Level 4.0 (1080p30).
    pub fn h264_4_0() -> CodecLevel {
        CodecLevel::new("4.0")
    }
    /// H.264 Level 4.1 (1080p30 + higher bitrate).
    pub fn h264_4_1() -> CodecLevel {
        CodecLevel::new("4.1")
    }
    /// H.264 Level 5.0 (1080p60 / 4K30).
    pub fn h264_5_0() -> CodecLevel {
        CodecLevel::new("5.0")
    }
    /// H.264 Level 5.1 (4K30).
    pub fn h264_5_1() -> CodecLevel {
        CodecLevel::new("5.1")
    }
    /// H.264 Level 5.2 (4K60).
    pub fn h264_5_2() -> CodecLevel {
        CodecLevel::new("5.2")
    }
}
