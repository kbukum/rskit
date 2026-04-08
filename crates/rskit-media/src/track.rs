//! Track information types for media containers.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{
    audio::{ChannelLayout, SampleRate},
    codec::{Codec, CodecLevel, CodecProfile},
    color::{ColorRange, ColorSpace, PixelFormat},
    spatial::{FrameRate, Resolution},
    types::TrackKind,
};

/// A single track/stream within a media container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    /// Track index within the container.
    pub index: usize,
    /// Kind of track.
    pub kind: TrackKind,
    /// Codec used for this track.
    pub codec: Option<Codec>,
    /// Track bitrate in bits per second.
    pub bitrate: Option<u64>,
    /// Track language (BCP 47 tag).
    pub language: Option<String>,
    /// Whether this is the default track for its kind.
    pub is_default: bool,
    /// Track title.
    pub title: Option<String>,
    /// Track duration.
    pub duration: Option<Duration>,
    /// Video-specific info (populated if `kind == Video`).
    pub video: Option<VideoTrackInfo>,
    /// Audio-specific info (populated if `kind == Audio`).
    pub audio: Option<AudioTrackInfo>,
    /// Subtitle-specific info (populated if `kind == Subtitle`).
    pub subtitle: Option<SubtitleTrackInfo>,
}

/// Video-specific track information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoTrackInfo {
    /// Resolution (width × height).
    pub resolution: Resolution,
    /// Frame rate.
    pub frame_rate: Option<FrameRate>,
    /// Pixel format.
    pub pixel_format: Option<PixelFormat>,
    /// Rotation in degrees (e.g., 90 for portrait video on mobile).
    pub rotation: Option<i16>,
    /// Color space.
    pub color_space: Option<ColorSpace>,
    /// Color range (limited / full).
    pub color_range: Option<ColorRange>,
    /// Bit depth per channel.
    pub bit_depth: Option<u8>,
    /// Codec profile (e.g., H264High, HevcMain10).
    pub profile: Option<CodecProfile>,
    /// Codec level (e.g., "4.1", "5.1").
    pub level: Option<CodecLevel>,
    /// HDR metadata (None for SDR content).
    pub hdr: Option<HdrMetadata>,
}

/// HDR metadata attached to a video track.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HdrMetadata {
    /// HDR format.
    pub format: HdrFormat,
    /// Mastering display metadata.
    pub mastering_display: Option<MasteringDisplay>,
    /// Content light level info.
    pub content_light_level: Option<ContentLightLevel>,
}

/// HDR format variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HdrFormat {
    /// HDR10 (static metadata).
    Hdr10,
    /// HDR10+ (dynamic metadata).
    Hdr10Plus,
    /// Dolby Vision.
    DolbyVision,
    /// Hybrid Log-Gamma.
    Hlg,
    /// Generic PQ (SMPTE ST 2084) without specific format.
    Pq,
}

/// SMPTE ST 2086 mastering display metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasteringDisplay {
    /// Display primaries as CIE 1931 chromaticity coordinates (x, y)
    /// in the order: Green, Blue, Red.
    pub primaries: Option<[(f64, f64); 3]>,
    /// White point as CIE 1931 chromaticity (x, y).
    pub white_point: Option<(f64, f64)>,
    /// Min/max luminance in cd/m² (nits).
    pub luminance: Option<(f64, f64)>,
}

/// Content light level info (CTA-861.3).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ContentLightLevel {
    /// Maximum Content Light Level in cd/m².
    pub max_cll: u32,
    /// Maximum Frame-Average Light Level in cd/m².
    pub max_fall: u32,
}

/// Audio-specific track information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioTrackInfo {
    /// Sample rate.
    pub sample_rate: SampleRate,
    /// Channel layout.
    pub channels: ChannelLayout,
    /// Bit depth per sample.
    pub bit_depth: Option<u8>,
}

/// Subtitle-specific track information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleTrackInfo {
    /// Subtitle format (e.g., "srt", "ass").
    pub format: String,
    /// Whether this is a forced subtitle track.
    pub forced: bool,
}
