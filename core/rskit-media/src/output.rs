//! Output configuration and encoding settings.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{
    audio::{ChannelLayout, SampleRate},
    codec::{Codec, CodecLevel, CodecProfile},
    format::Format,
    registry::Registry,
    spatial::{FrameRate, Resolution},
};
use rskit_errors::{AppError, AppResult, ErrorCode};

/// Encoding quality preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Quality {
    /// Lossless encoding.
    Lossless,
    /// Ultra-high quality.
    UltraHigh,
    /// High quality.
    High,
    /// Medium quality (default).
    Medium,
    /// Low quality.
    Low,
    /// Very low quality.
    VeryLow,
    /// Custom CRF/quality value (0–51 for x264).
    Custom(u8),
}

/// Bitrate specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Bitrate {
    /// Constant bitrate (bits/sec).
    Constant(u64),
    /// Variable bitrate target (bits/sec).
    Variable(u64),
    /// Constrained variable bitrate.
    Constrained {
        /// Target bitrate.
        target: u64,
        /// Maximum bitrate.
        max: u64,
    },
}

/// Encoding speed/effort tradeoff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EncodingSpeed {
    /// Fastest encoding, lowest quality.
    UltraFast,
    /// Very fast encoding.
    SuperFast,
    /// Fast encoding.
    VeryFast,
    /// Faster than medium.
    Fast,
    /// Balanced speed/quality.
    Medium,
    /// Slower encoding, better quality.
    Slow,
    /// Slowest encoding, best quality.
    VerySlow,
}

/// Video-specific encoding settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoSettings {
    /// Video codec.
    pub codec: Codec,
    /// Output resolution.
    pub resolution: Option<Resolution>,
    /// Output frame rate.
    pub frame_rate: Option<FrameRate>,
    /// Quality preset.
    pub quality: Option<Quality>,
    /// Bitrate setting.
    pub bitrate: Option<Bitrate>,
    /// Encoding speed.
    pub speed: Option<EncodingSpeed>,
    /// Codec profile (e.g., H264High, HevcMain10).
    pub profile: Option<CodecProfile>,
    /// Codec level (e.g., "4.1").
    pub level: Option<CodecLevel>,
}

impl VideoSettings {
    /// Create new video settings with the given codec.
    pub fn new(codec: Codec) -> Self {
        Self {
            codec,
            resolution: None,
            frame_rate: None,
            quality: None,
            bitrate: None,
            speed: None,
            profile: None,
            level: None,
        }
    }

    /// Set the output resolution.
    #[must_use]
    pub fn with_resolution(mut self, res: Resolution) -> Self {
        self.resolution = Some(res);
        self
    }

    /// Set the output frame rate.
    #[must_use]
    pub fn with_frame_rate(mut self, fps: FrameRate) -> Self {
        self.frame_rate = Some(fps);
        self
    }

    /// Set the quality preset.
    #[must_use]
    pub fn with_quality(mut self, q: Quality) -> Self {
        self.quality = Some(q);
        self
    }

    /// Set the bitrate.
    #[must_use]
    pub fn with_bitrate(mut self, br: Bitrate) -> Self {
        self.bitrate = Some(br);
        self
    }

    /// Set the encoding speed.
    #[must_use]
    pub fn with_speed(mut self, speed: EncodingSpeed) -> Self {
        self.speed = Some(speed);
        self
    }

    /// Set the codec profile.
    #[must_use]
    pub fn with_profile(mut self, profile: CodecProfile) -> Self {
        self.profile = Some(profile);
        self
    }

    /// Set the codec level.
    #[must_use]
    pub fn with_level(mut self, level: CodecLevel) -> Self {
        self.level = Some(level);
        self
    }
}

/// Audio-specific encoding settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioSettings {
    /// Audio codec.
    pub codec: Codec,
    /// Sample rate.
    pub sample_rate: Option<SampleRate>,
    /// Channel layout.
    pub channels: Option<ChannelLayout>,
    /// Bitrate.
    pub bitrate: Option<Bitrate>,
}

impl AudioSettings {
    /// Create new audio settings with the given codec.
    pub fn new(codec: Codec) -> Self {
        Self {
            codec,
            sample_rate: None,
            channels: None,
            bitrate: None,
        }
    }

    /// Set the sample rate.
    #[must_use]
    pub fn with_sample_rate(mut self, sr: SampleRate) -> Self {
        self.sample_rate = Some(sr);
        self
    }

    /// Set the channel layout.
    #[must_use]
    pub fn with_channels(mut self, ch: ChannelLayout) -> Self {
        self.channels = Some(ch);
        self
    }

    /// Set the bitrate.
    #[must_use]
    pub fn with_bitrate(mut self, br: Bitrate) -> Self {
        self.bitrate = Some(br);
        self
    }
}

/// Complete output configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    /// Output container format.
    pub format: Format,
    /// Video encoding settings (None for audio-only).
    pub video: Option<VideoSettings>,
    /// Audio encoding settings (None for video-only).
    pub audio: Option<AudioSettings>,
    /// Streaming output settings (HLS, DASH, RTMP).
    pub streaming: Option<StreamingConfig>,
    /// Whether to strip metadata from output.
    pub strip_metadata: bool,
    /// Extra backend-specific parameters.
    pub extra: HashMap<String, String>,
}

/// Streaming output configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamingConfig {
    /// HTTP Live Streaming (HLS) output.
    Hls(HlsConfig),
    /// MPEG-DASH output.
    Dash(DashConfig),
    /// RTMP push output.
    Rtmp(RtmpConfig),
}

/// HLS output configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HlsConfig {
    /// Segment duration in seconds (default: 6).
    pub segment_duration: u32,
    /// Number of segments in playlist (0 = all).
    pub playlist_size: u32,
    /// Playlist type.
    pub playlist_type: HlsPlaylistType,
    /// Segment filename pattern (default: "segment_%03d.ts").
    pub segment_filename: Option<String>,
}

impl Default for HlsConfig {
    fn default() -> Self {
        Self {
            segment_duration: 6,
            playlist_size: 0,
            playlist_type: HlsPlaylistType::Vod,
            segment_filename: None,
        }
    }
}

/// HLS playlist type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HlsPlaylistType {
    /// Video on demand — all segments in playlist.
    Vod,
    /// Live/event — sliding window.
    Event,
}

/// DASH output configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashConfig {
    /// Segment duration in seconds (default: 4).
    pub segment_duration: u32,
    /// Use segment template mode.
    pub use_template: bool,
    /// Use segment timeline.
    pub use_timeline: bool,
}

impl Default for DashConfig {
    fn default() -> Self {
        Self {
            segment_duration: 4,
            use_template: true,
            use_timeline: true,
        }
    }
}

/// RTMP push configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RtmpConfig {
    /// RTMP server URL (e.g., "rtmp://live.example.com/app/stream_key").
    pub url: String,
}

impl OutputConfig {
    /// Create a new output config with the given format.
    pub fn new(format: Format) -> Self {
        Self {
            format,
            video: None,
            audio: None,
            streaming: None,
            strip_metadata: false,
            extra: HashMap::new(),
        }
    }

    /// Set video encoding settings.
    #[must_use]
    pub fn with_video(mut self, video: VideoSettings) -> Self {
        self.video = Some(video);
        self
    }

    /// Set audio encoding settings.
    #[must_use]
    pub fn with_audio(mut self, audio: AudioSettings) -> Self {
        self.audio = Some(audio);
        self
    }

    /// Set streaming output configuration.
    #[must_use]
    pub fn with_streaming(mut self, streaming: StreamingConfig) -> Self {
        self.streaming = Some(streaming);
        self
    }

    /// Strip metadata from output.
    #[must_use]
    pub fn with_strip_metadata(mut self) -> Self {
        self.strip_metadata = true;
        self
    }

    /// Add an extra backend-specific parameter.
    #[must_use]
    pub fn with_param(mut self, key: impl Into<String>, val: impl Into<String>) -> Self {
        self.extra.insert(key.into(), val.into());
        self
    }

    /// Validate codec/format compatibility against a registry.
    pub fn validate(&self, registry: &Registry) -> AppResult<()> {
        if let Some(video) = &self.video
            && !registry.is_compatible(&video.codec, &self.format)
        {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!(
                    "video codec {} is not compatible with format {}",
                    video.codec, self.format,
                ),
            ));
        }
        if let Some(audio) = &self.audio
            && !registry.is_compatible(&audio.codec, &self.format)
        {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!(
                    "audio codec {} is not compatible with format {}",
                    audio.codec, self.format,
                ),
            ));
        }
        Ok(())
    }
}
