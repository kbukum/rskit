//! Extended probe analysis types — keyframes, silence, chapters.
//!
//! These types represent structural information about media that goes beyond
//! basic metadata. They power chunking strategies, timeline UIs, and
//! intelligent splitting.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::time::{TimeRange, Timestamp};

/// A keyframe (I-frame) position within a video stream.
///
/// Keyframes are the only safe points to split video without re-encoding.
/// Used by [`ChunkStrategy`](crate::chunking::ChunkStrategy) implementations
/// to plan efficient parallel processing boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct KeyframeInfo {
    /// Position in the stream.
    pub timestamp: Timestamp,
    /// Frame number (zero-based).
    pub frame_number: u64,
    /// Picture type from the codec (I, IDR, etc.).
    pub picture_type: PictureType,
    /// Packet size in bytes (useful for bitrate analysis).
    pub size_bytes: Option<u64>,
}

/// Video picture type from the codec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum PictureType {
    /// Intra-coded frame (full frame, no references).
    I,
    /// IDR frame (instantaneous decoder refresh — resets reference list).
    Idr,
    /// Predicted frame (references past frames).
    P,
    /// Bi-predicted frame (references past and future frames).
    B,
    /// Unknown or unrecognized type.
    Unknown,
}

impl PictureType {
    /// Parse from ffprobe's `pict_type` field.
    #[must_use]
    pub fn from_ffprobe(s: &str) -> Self {
        match s.trim() {
            "I" => Self::I,
            "IDR" | "idr" => Self::Idr,
            "P" => Self::P,
            "B" => Self::B,
            _ => Self::Unknown,
        }
    }

    /// Whether this picture type is a keyframe (safe split point).
    #[must_use]
    pub fn is_keyframe(&self) -> bool {
        matches!(self, Self::I | Self::Idr)
    }
}

/// A detected silence interval within an audio stream.
///
/// Silence intervals are optimal split points for audio-centric operations
/// like transcription — splitting at silence avoids cutting words.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SilenceInterval {
    /// Start of the silence.
    pub start: Timestamp,
    /// End of the silence.
    pub end: Timestamp,
    /// Duration of the silence.
    pub duration: Duration,
}

impl SilenceInterval {
    /// The midpoint of this silence interval — ideal split point.
    #[must_use]
    pub fn midpoint(&self) -> Timestamp {
        let mid_us = (self.start.as_micros() + self.end.as_micros()) / 2;
        Timestamp::from_micros(mid_us)
    }

    /// Convert to a [`TimeRange`].
    #[must_use]
    pub fn as_range(&self) -> TimeRange {
        TimeRange::new(self.start, self.end)
    }
}

/// A chapter marker embedded in the media container.
///
/// Chapters provide semantic structure (intro, verse, chorus, etc.)
/// and are common in podcasts, audiobooks, and long-form video.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chapter {
    /// Chapter index (zero-based).
    pub index: usize,
    /// Time range of this chapter.
    pub range: TimeRange,
    /// Chapter title (from container metadata).
    pub title: Option<String>,
}

impl Chapter {
    /// Duration of this chapter.
    #[must_use]
    pub fn duration(&self) -> Duration {
        self.range.duration()
    }
}
