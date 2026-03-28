//! Track information types for media containers.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{
    audio::{ChannelLayout, SampleRate},
    codec::Codec,
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
    /// Pixel format (e.g., "yuv420p").
    pub pixel_format: Option<String>,
    /// Rotation in degrees (e.g., 90 for portrait video on mobile).
    pub rotation: Option<i16>,
    /// Color space (e.g., "bt709").
    pub color_space: Option<String>,
    /// Bit depth per channel.
    pub bit_depth: Option<u8>,
    /// Whether the video uses HDR.
    pub hdr: bool,
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
