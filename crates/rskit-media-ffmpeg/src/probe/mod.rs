//! FFmpeg probe implementation — media analysis via `ffprobe` and `ffmpeg`.
//!
//! Organized into focused sub-modules:
//! - [`parse`] — FFprobe JSON → [`MediaMetadata`] conversion
//! - [`thumbnail`] — Thumbnail and visual extraction
//! - [`detect`] — Scene, keyframe, silence, and chapter detection

mod detect;
mod parse;
mod thumbnail;

use std::time::Duration;

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_file::FileSource;
use rskit_media::{
    probe::{Chapter, KeyframeInfo, MediaMetadata, MediaProbe, SilenceInterval},
    spatial::Resolution,
    time::Timestamp,
};

use crate::config::FfmpegConfig;

/// FFmpeg-based media probe using `ffprobe`.
pub struct FfmpegProbe {
    config: FfmpegConfig,
}

impl FfmpegProbe {
    /// Create a new probe with the given configuration.
    pub fn new(config: FfmpegConfig) -> Self {
        Self { config }
    }

    /// Check that ffprobe is available and return its version.
    pub async fn check_available(&self) -> AppResult<String> {
        let output = tokio::process::Command::new(self.config.ffprobe_bin())
            .arg("-version")
            .output()
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::ServiceUnavailable,
                    format!("ffprobe not found: {e}"),
                )
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let version = stdout.lines().next().unwrap_or("unknown").to_string();
        Ok(version)
    }

    /// Run ffprobe and return the raw JSON output.
    pub async fn probe_raw(&self, source: &FileSource) -> AppResult<serde_json::Value> {
        let resolved = source.to_local_path().await?;
        let path = resolved.path();

        let output = tokio::process::Command::new(self.config.ffprobe_bin())
            .args([
                "-v",
                "quiet",
                "-print_format",
                "json",
                "-show_format",
                "-show_streams",
                "-show_chapters",
            ])
            .arg(path)
            .output()
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("ffprobe execution failed: {e}"),
                )
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::new(
                ErrorCode::Internal,
                format!("ffprobe failed: {stderr}"),
            ));
        }

        let json: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("ffprobe output is not valid JSON: {e}"),
            )
        })?;

        Ok(json)
    }
}

// ── MediaProbe trait — delegates to focused sub-modules ─────────────────────

#[async_trait::async_trait]
impl MediaProbe for FfmpegProbe {
    async fn probe(&self, source: &FileSource) -> AppResult<MediaMetadata> {
        let json = self.probe_raw(source).await?;
        parse::parse_metadata(&json)
    }

    async fn thumbnail(
        &self,
        source: &FileSource,
        at: Timestamp,
        resolution: Option<Resolution>,
    ) -> AppResult<FileSource> {
        self.extract_thumbnail(source, at, resolution).await
    }

    async fn thumbnails(
        &self,
        source: &FileSource,
        interval: Duration,
        resolution: Option<Resolution>,
    ) -> AppResult<Vec<FileSource>> {
        self.extract_thumbnails(source, interval, resolution).await
    }

    async fn sprite_sheet(
        &self,
        source: &FileSource,
        interval: Duration,
        thumb_resolution: Resolution,
        columns: u32,
    ) -> AppResult<FileSource> {
        self.extract_sprite_sheet(source, interval, thumb_resolution, columns)
            .await
    }

    async fn scene_detect(&self, source: &FileSource, threshold: f64) -> AppResult<Vec<Timestamp>> {
        self.detect_scenes(source, threshold).await
    }

    async fn waveform(&self, source: &FileSource, resolution: Resolution) -> AppResult<FileSource> {
        self.extract_waveform(source, resolution).await
    }

    async fn keyframes(&self, source: &FileSource) -> AppResult<Vec<KeyframeInfo>> {
        self.extract_keyframes(source).await
    }

    async fn silence_detect(
        &self,
        source: &FileSource,
        min_duration: Duration,
        noise_threshold_db: f64,
    ) -> AppResult<Vec<SilenceInterval>> {
        self.detect_silence(source, min_duration, noise_threshold_db)
            .await
    }

    async fn chapters(&self, source: &FileSource) -> AppResult<Vec<Chapter>> {
        self.extract_chapters(source).await
    }
}
