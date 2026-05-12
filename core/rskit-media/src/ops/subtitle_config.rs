//! Configuration types for the `AddSubtitles` operation.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::subtitle::SubtitleStyle;

/// Configuration for burning subtitles into video.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleConfig {
    /// Where the subtitle data comes from.
    pub source: SubtitleSource,
    /// Subtitle file format.
    pub format: SubtitleFormat,
    /// Optional visual styling overrides.
    pub style: Option<SubtitleStyle>,
}

/// Source of subtitle data.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum SubtitleSource {
    /// Load from a file path.
    File(PathBuf),
    /// Inline subtitle content as a string.
    Inline(String),
}

/// Subtitle file format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum SubtitleFormat {
    /// SubRip (.srt).
    Srt,
    /// WebVTT (.vtt).
    Vtt,
    /// Advanced SubStation Alpha (.ass).
    Ass,
}

impl SubtitleFormat {
    /// File extension for this format.
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Srt => "srt",
            Self::Vtt => "vtt",
            Self::Ass => "ass",
        }
    }
}
