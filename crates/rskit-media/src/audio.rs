//! Audio types for media processing.

use serde::{Deserialize, Serialize};

/// Audio sample rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SampleRate(pub u32);

impl SampleRate {
    /// Create from Hz value.
    pub fn hz(n: u32) -> Self { Self(n) }
    /// CD quality (44,100 Hz).
    pub fn cd() -> Self { Self(44100) }
    /// DVD quality (48,000 Hz).
    pub fn dvd() -> Self { Self(48000) }
    /// HD audio (96,000 Hz).
    pub fn hd() -> Self { Self(96000) }
}

/// Audio channel layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChannelLayout {
    /// Single channel.
    Mono,
    /// Two channels (left + right).
    Stereo,
    /// 5.1 surround sound.
    Surround51,
    /// 7.1 surround sound.
    Surround71,
    /// Custom number of channels.
    Custom(u16),
}

impl ChannelLayout {
    /// Number of channels in this layout.
    pub fn channel_count(&self) -> u16 {
        match self {
            Self::Mono => 1,
            Self::Stereo => 2,
            Self::Surround51 => 6,
            Self::Surround71 => 8,
            Self::Custom(n) => *n,
        }
    }
}
