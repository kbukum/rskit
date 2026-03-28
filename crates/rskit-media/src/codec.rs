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
