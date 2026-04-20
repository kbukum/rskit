//! Core media metadata types returned by probing.

use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    audio::SampleRate,
    format::Format,
    spatial::{FrameRate, Resolution},
    track::Track,
    types::{MediaType, TrackKind},
};

/// Full probe result — everything knowable about a media file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaMetadata {
    /// The broad media type.
    pub media_type: MediaType,
    /// Container format.
    pub format: Format,
    /// Total duration.
    pub duration: Option<Duration>,
    /// File size in bytes.
    pub size: Option<u64>,
    /// Overall bitrate in bits/sec.
    pub bitrate: Option<u64>,
    /// All tracks in the container.
    pub tracks: Vec<Track>,
    /// File-level tags/metadata.
    pub tags: HashMap<String, String>,
    /// Creation date if available.
    pub created_at: Option<DateTime<Utc>>,
}

impl MediaMetadata {
    /// Get the first video track.
    pub fn video_track(&self) -> Option<&Track> {
        self.tracks.iter().find(|t| t.kind == TrackKind::Video)
    }

    /// Get the first audio track.
    pub fn audio_track(&self) -> Option<&Track> {
        self.tracks.iter().find(|t| t.kind == TrackKind::Audio)
    }

    /// Get all subtitle tracks.
    pub fn subtitle_tracks(&self) -> Vec<&Track> {
        self.tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Subtitle)
            .collect()
    }

    /// Resolution from the first video track.
    pub fn resolution(&self) -> Option<Resolution> {
        self.video_track()
            .and_then(|t| t.video.as_ref())
            .map(|v| v.resolution)
    }

    /// Frame rate from the first video track.
    pub fn frame_rate(&self) -> Option<FrameRate> {
        self.video_track()
            .and_then(|t| t.video.as_ref())
            .and_then(|v| v.frame_rate)
    }

    /// Sample rate from the first audio track.
    pub fn sample_rate(&self) -> Option<SampleRate> {
        self.audio_track()
            .and_then(|t| t.audio.as_ref())
            .map(|a| a.sample_rate)
    }

    /// Whether the file has at least one video track.
    pub fn has_video(&self) -> bool {
        self.video_track().is_some()
    }

    /// Whether the file has at least one audio track.
    pub fn has_audio(&self) -> bool {
        self.audio_track().is_some()
    }
}
