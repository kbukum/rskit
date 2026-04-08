//! Media metadata and probe trait.

use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use rskit_errors::AppResult;
use rskit_file::FileSource;
use serde::{Deserialize, Serialize};

use crate::{
    audio::SampleRate,
    format::Format,
    spatial::{FrameRate, Resolution},
    time::Timestamp,
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

/// Inspect media files — extract metadata without processing.
#[async_trait::async_trait]
pub trait MediaProbe: Send + Sync {
    /// Probe a media file and return its metadata.
    async fn probe(&self, source: &FileSource) -> AppResult<MediaMetadata>;

    /// Extract a single thumbnail at a given timestamp.
    async fn thumbnail(
        &self,
        source: &FileSource,
        at: Timestamp,
        resolution: Option<Resolution>,
    ) -> AppResult<FileSource>;

    /// Extract thumbnails at regular intervals.
    async fn thumbnails(
        &self,
        source: &FileSource,
        interval: Duration,
        resolution: Option<Resolution>,
    ) -> AppResult<Vec<FileSource>>;

    /// Generate a thumbnail sprite sheet (contact sheet).
    ///
    /// Returns a single image containing a grid of thumbnails at regular
    /// intervals. Useful for video scrubbing UIs.
    async fn sprite_sheet(
        &self,
        _source: &FileSource,
        _interval: Duration,
        _thumb_resolution: Resolution,
        _columns: u32,
    ) -> AppResult<FileSource> {
        Err(rskit_errors::AppError::new(
            rskit_errors::ErrorCode::InvalidInput,
            "sprite_sheet not supported by this backend",
        ))
    }

    /// Detect scene changes and return their timestamps.
    ///
    /// `threshold` is 0.0–1.0 where lower values detect more scenes.
    /// Typical values: 0.3 (sensitive) to 0.5 (conservative).
    async fn scene_detect(
        &self,
        _source: &FileSource,
        _threshold: f64,
    ) -> AppResult<Vec<Timestamp>> {
        Err(rskit_errors::AppError::new(
            rskit_errors::ErrorCode::InvalidInput,
            "scene_detect not supported by this backend",
        ))
    }

    /// Generate an audio waveform image.
    ///
    /// Returns a PNG image of the audio waveform.
    async fn waveform(
        &self,
        _source: &FileSource,
        _resolution: Resolution,
    ) -> AppResult<FileSource> {
        Err(rskit_errors::AppError::new(
            rskit_errors::ErrorCode::InvalidInput,
            "waveform not supported by this backend",
        ))
    }
}
