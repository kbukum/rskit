//! Container/file format identifiers and well-known constants.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// An open container/file format identifier.
///
/// # Examples
///
/// ```rust
/// use rskit_media::format::{self, Format};
///
/// let mp4 = Format::new(format::MP4);
/// let custom = Format::new("my_container");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Format(Arc<str>);

impl Format {
    /// Create a new format identifier.
    pub fn new(id: impl Into<Arc<str>>) -> Self {
        Self(id.into())
    }

    /// The format identifier string.
    pub fn id(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ── Video containers ─────────────────────────────────────────────────

/// MPEG-4 Part 14.
pub const MP4: &str = "mp4";
/// Matroska.
pub const MKV: &str = "mkv";
/// WebM.
pub const WEBM: &str = "webm";
/// Audio Video Interleave.
pub const AVI: &str = "avi";
/// QuickTime.
pub const MOV: &str = "mov";
/// Flash Video.
pub const FLV: &str = "flv";
/// MPEG Transport Stream.
pub const TS: &str = "ts";
/// MPEG-4 Video.
pub const M4V: &str = "m4v";
/// Windows Media Video.
pub const WMV: &str = "wmv";

// ── Audio containers ─────────────────────────────────────────────────

/// MP3 file.
pub const MP3: &str = "mp3";
/// Waveform Audio.
pub const WAV: &str = "wav";
/// Free Lossless Audio Codec file.
pub const FLAC: &str = "flac";
/// Ogg container.
pub const OGG: &str = "ogg";
/// AAC file.
pub const AAC: &str = "aac";
/// MPEG-4 Audio.
pub const M4A: &str = "m4a";
/// Windows Media Audio file.
pub const WMA: &str = "wma";
/// Opus file.
pub const OPUS: &str = "opus";

// ── Image formats ────────────────────────────────────────────────────

/// PNG image.
pub const PNG: &str = "png";
/// JPEG image.
pub const JPEG: &str = "jpeg";
/// WebP image.
pub const WEBP: &str = "webp";
/// GIF image.
pub const GIF: &str = "gif";
/// BMP image.
pub const BMP: &str = "bmp";
/// TIFF image.
pub const TIFF: &str = "tiff";
/// SVG image.
pub const SVG: &str = "svg";
/// AVIF image.
pub const AVIF: &str = "avif";
/// HEIF image.
pub const HEIF: &str = "heif";

// ── Subtitle formats ─────────────────────────────────────────────────

/// SubRip.
pub const SRT: &str = "srt";
/// WebVTT.
pub const VTT: &str = "vtt";
/// Advanced SubStation Alpha.
pub const ASS: &str = "ass";
