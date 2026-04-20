//! Configuration types for the `GenerateThumbnail` operation.

use serde::{Deserialize, Serialize};

/// Configuration for extracting a single frame as an image.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThumbnailConfig {
    /// Timestamp in seconds at which to extract the frame.
    pub timestamp: f64,
    /// Optional output width (preserves aspect ratio when only one dimension is set).
    pub width: Option<u32>,
    /// Optional output height.
    pub height: Option<u32>,
    /// Output image format.
    pub format: ImageFormat,
    /// Quality (1–100) for JPEG/WebP output.
    pub quality: Option<u8>,
}

/// Output image format for thumbnails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ImageFormat {
    /// JPEG format.
    Jpeg,
    /// PNG format (lossless).
    Png,
    /// WebP format.
    Webp,
}

impl ImageFormat {
    /// FFmpeg-compatible codec name for this format.
    pub fn ffmpeg_codec(&self) -> &'static str {
        match self {
            Self::Jpeg => "mjpeg",
            Self::Png => "png",
            Self::Webp => "libwebp",
        }
    }

    /// File extension for this format.
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::Webp => "webp",
        }
    }
}
