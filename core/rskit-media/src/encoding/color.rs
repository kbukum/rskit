//! Color space, color range, and pixel format types.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// Color space (color primaries + matrix coefficients).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ColorSpace {
    /// ITU-R BT.601 (SD video).
    Bt601,
    /// ITU-R BT.709 (HD video, sRGB).
    Bt709,
    /// ITU-R BT.2020 (UHD/HDR video).
    Bt2020,
    /// SMPTE 240M (legacy HDTV).
    Smpte240m,
    /// sRGB (images, web).
    Srgb,
    /// DCI-P3 (cinema, wide gamut displays).
    DciP3,
    /// Display P3 (Apple displays).
    DisplayP3,
    /// Unknown or unrecognized color space.
    Unknown,
}

impl ColorSpace {
    /// Convert to FFmpeg color space string.
    pub fn as_ffmpeg_arg(&self) -> Option<&str> {
        match self {
            Self::Bt601 => Some("bt470bg"),
            Self::Bt709 => Some("bt709"),
            Self::Bt2020 => Some("bt2020nc"),
            Self::Smpte240m => Some("smpte240m"),
            Self::Srgb => Some("bt709"),
            Self::DciP3 => Some("bt2020nc"),
            Self::DisplayP3 => Some("bt2020nc"),
            Self::Unknown => None,
        }
    }

    /// Parse from FFmpeg colorspace string.
    pub fn from_ffmpeg(s: &str) -> Self {
        match s {
            "bt709" => Self::Bt709,
            "bt470bg" | "smpte170m" => Self::Bt601,
            "bt2020nc" | "bt2020c" => Self::Bt2020,
            "smpte240m" => Self::Smpte240m,
            _ => Self::Unknown,
        }
    }
}

/// Color range (signal levels).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ColorRange {
    /// Limited range (16–235 for 8-bit luma).
    Limited,
    /// Full range (0–255 for 8-bit luma).
    Full,
}

impl ColorRange {
    /// Convert to FFmpeg color range string.
    pub fn as_ffmpeg_arg(&self) -> &str {
        match self {
            Self::Limited => "tv",
            Self::Full => "pc",
        }
    }

    /// Parse from FFmpeg color range string.
    pub fn from_ffmpeg(s: &str) -> Option<Self> {
        match s {
            "tv" | "limited" | "mpeg" => Some(Self::Limited),
            "pc" | "full" | "jpeg" => Some(Self::Full),
            _ => None,
        }
    }
}

/// Open pixel format identifier (e.g., "yuv420p", "rgb24", "nv12").
///
/// Like [`Codec`](crate::codec::Codec) and [`Format`](crate::format::Format),
/// this is an open identifier with well-known constants.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PixelFormat(Arc<str>);

impl PixelFormat {
    /// Create a new pixel format identifier.
    pub fn new(id: impl Into<Arc<str>>) -> Self {
        Self(id.into())
    }

    /// The pixel format identifier string.
    pub fn id(&self) -> &str {
        &self.0
    }

    /// Bit depth per component (approximate, based on well-known formats).
    pub fn bit_depth(&self) -> Option<u8> {
        match self.0.as_ref() {
            "yuv420p" | "yuv422p" | "yuv444p" | "rgb24" | "bgr24" | "nv12" | "nv21" | "yuyv422"
            | "uyvy422" => Some(8),
            "yuv420p10le" | "yuv422p10le" | "yuv444p10le" | "p010le" => Some(10),
            "yuv420p12le" | "yuv422p12le" | "yuv444p12le" => Some(12),
            "rgb48le" | "yuv444p16le" => Some(16),
            _ => None,
        }
    }

    /// Whether this format has an alpha channel.
    pub fn has_alpha(&self) -> bool {
        matches!(
            self.0.as_ref(),
            "rgba" | "bgra" | "argb" | "abgr" | "yuva420p" | "yuva444p"
        )
    }
}

impl std::fmt::Display for PixelFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Well-known pixel formats.
pub mod pixel_formats {
    use super::PixelFormat;

    /// YUV 4:2:0 planar, 8-bit (most common video format).
    pub fn yuv420p() -> PixelFormat {
        PixelFormat::new("yuv420p")
    }
    /// YUV 4:2:2 planar, 8-bit.
    pub fn yuv422p() -> PixelFormat {
        PixelFormat::new("yuv422p")
    }
    /// YUV 4:4:4 planar, 8-bit.
    pub fn yuv444p() -> PixelFormat {
        PixelFormat::new("yuv444p")
    }
    /// YUV 4:2:0 planar, 10-bit little-endian (HDR).
    pub fn yuv420p10le() -> PixelFormat {
        PixelFormat::new("yuv420p10le")
    }
    /// YUV 4:2:0 planar, 12-bit little-endian.
    pub fn yuv420p12le() -> PixelFormat {
        PixelFormat::new("yuv420p12le")
    }
    /// RGB packed, 8-bit per channel.
    pub fn rgb24() -> PixelFormat {
        PixelFormat::new("rgb24")
    }
    /// RGBA packed, 8-bit per channel.
    pub fn rgba() -> PixelFormat {
        PixelFormat::new("rgba")
    }
    /// NV12 (YUV 4:2:0 semi-planar, common for hw decoders).
    pub fn nv12() -> PixelFormat {
        PixelFormat::new("nv12")
    }
    /// P010LE (10-bit NV12, common for HDR hw decoders).
    pub fn p010le() -> PixelFormat {
        PixelFormat::new("p010le")
    }
}
