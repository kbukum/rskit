//! FFmpeg probe implementation — media analysis via `ffprobe` and `ffmpeg`.
//!
//! Organized into focused sub-modules:
//! - [`parse`] — FFprobe JSON → [`MediaMetadata`] conversion
//! - [`thumbnail`] — Thumbnail and visual extraction
//! - [`detect`] — Scene, keyframe, silence, and chapter detection

mod detect;
mod parse;
mod thumbnail;

use std::ffi::OsString;
use std::time::Duration;

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_media::{
    probe::{Chapter, KeyframeInfo, MediaMetadata, MediaProbe, SilenceInterval},
    spatial::Resolution,
    time::Timestamp,
};
use rskit_storage::FileSource;

use crate::config::FfmpegConfig;
use crate::process::{ensure_success, run_capture, with_context};

/// FFmpeg-based media probe using `ffprobe`.
pub(crate) struct FfmpegProbe {
    config: FfmpegConfig,
}

impl FfmpegProbe {
    /// Create a new probe with the given configuration.
    pub(crate) fn new(config: FfmpegConfig) -> Self {
        Self { config }
    }

    /// Run ffprobe and return the raw JSON output.
    pub(crate) async fn probe_raw(&self, source: &FileSource) -> AppResult<serde_json::Value> {
        let resolved = source.to_local_path().await?;
        let path = crate::paths::resolved_source_path(&self.config, source, resolved.path())?;

        let output = run_capture(
            self.config.ffprobe_bin(),
            vec![
                OsString::from("-v"),
                OsString::from("quiet"),
                OsString::from("-print_format"),
                OsString::from("json"),
                OsString::from("-show_format"),
                OsString::from("-show_streams"),
                OsString::from("-show_chapters"),
                path.as_os_str().to_os_string(),
            ],
            self.config.timeout,
        )
        .await
        .map_err(|e| with_context(e, "ffprobe execution failed"))?;

        ensure_success(&output, "ffprobe")?;

        let json: serde_json::Value =
            serde_json::from_slice(&output.stdout_bytes).map_err(|e| {
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
