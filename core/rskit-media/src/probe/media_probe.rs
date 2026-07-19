use std::time::Duration;

use rskit_errors::AppResult;
use rskit_storage::FileSource;

use crate::spatial::Resolution;
use crate::time::Timestamp;

use super::{Chapter, KeyframeInfo, MediaMetadata, SilenceInterval};

/// Inspect media files — extract metadata without processing.
///
/// Backends implement this trait to provide media analysis capabilities.
/// All methods except [`probe`](MediaProbe::probe) have default implementations that return "not supported" errors,
/// so backends can implement only what they support.
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
    /// Returns a single image containing a grid of thumbnails at regular intervals.
    /// Useful for video scrubbing UIs.
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
    /// `threshold` is 0.0–1.0 where lower values detect more scenes. Typical values:
    /// 0.3 (sensitive) to 0.5 (conservative).
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

    /// Extract keyframe (I-frame) positions from a video stream.
    ///
    /// Returns all keyframe timestamps, ordered chronologically.
    /// Used by chunking strategies to find safe split points.
    async fn keyframes(&self, _source: &FileSource) -> AppResult<Vec<KeyframeInfo>> {
        Err(rskit_errors::AppError::new(
            rskit_errors::ErrorCode::InvalidInput,
            "keyframes not supported by this backend",
        ))
    }

    /// Detect silence intervals in the audio stream.
    ///
    /// * `min_duration` — Minimum silence length to report (e.g., 0.5s).
    /// * `noise_threshold_db` — dB threshold below which audio is considered silence. Typical:
    ///   -30 to -50 dB.
    async fn silence_detect(
        &self,
        _source: &FileSource,
        _min_duration: Duration,
        _noise_threshold_db: f64,
    ) -> AppResult<Vec<SilenceInterval>> {
        Err(rskit_errors::AppError::new(
            rskit_errors::ErrorCode::InvalidInput,
            "silence_detect not supported by this backend",
        ))
    }

    /// Extract chapter markers from the media container.
    ///
    /// Chapters are embedded metadata in formats like MP4, MKV, and OGG.
    /// Returns an empty vec if the container has no chapters.
    async fn chapters(&self, _source: &FileSource) -> AppResult<Vec<Chapter>> {
        Err(rskit_errors::AppError::new(
            rskit_errors::ErrorCode::InvalidInput,
            "chapters not supported by this backend",
        ))
    }
}
