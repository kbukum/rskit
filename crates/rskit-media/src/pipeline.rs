//! Lazy chainable media pipeline builder.

use std::time::Duration;

use rskit_errors::AppResult;
use rskit_storage::{FileSink, FileSource};
use tokio_util::sync::CancellationToken;

use crate::{
    executor::MediaExecutor,
    filter::Filter,
    ops::{
        ConcatOp, CropRegion, FilterConfig, FlipDirection, InterpolateConfig, MediaOp, MixAudioOp,
        OverlayConfig, OverlayOp, OverlayPosition, PadOp, ReplaceAudioOp, ResizeMode, ResizeOp,
        Rotation, SceneDetectConfig, SubtitleConfig, ThumbnailConfig, Transition, UpscaleConfig,
    },
    output::OutputConfig,
    spatial::Resolution,
    subtitle::SubtitleTrack,
    time::{Segment, TimeRange, Timestamp},
    types::TrackKind,
};

/// A lazy pipeline of media operations.
///
/// Operations are not executed when chained — they are recorded.
/// Call [`.execute()`](MediaPipeline::execute) to compile and run the full pipeline.
///
/// # Example
///
/// ```rust,ignore
/// use rskit_media::{presets, filter::filters};
///
/// let result = MediaPipeline::from(&source)
///     .extract(TimeRange::from_seconds(10.0, 60.0))
///     .resize(Resolution::p1080(), ResizeMode::Fit)
///     .filter(filters::denoise(3))
///     .volume(0.8)
///     .speed(1.25)
///     .transcode(presets::mp4_h264())
///     .execute(&executor)
///     .await?;
/// ```
pub struct MediaPipeline {
    source: FileSource,
    ops: Vec<MediaOp>,
    sink: Option<FileSink>,
}

impl MediaPipeline {
    // ── Construction ─────────────────────────────────────────────────

    /// Create a pipeline from a file source.
    pub fn from(source: &FileSource) -> Self {
        Self {
            source: source.clone(),
            ops: Vec::new(),
            sink: None,
        }
    }

    // ── Temporal ─────────────────────────────────────────────────────

    /// Extract a time range.
    #[must_use]
    pub fn extract(mut self, range: TimeRange) -> Self {
        self.ops.push(MediaOp::Extract(range));
        self
    }

    /// Extract multiple segments.
    #[must_use]
    pub fn extract_many(mut self, segments: Vec<Segment>) -> Self {
        self.ops.push(MediaOp::ExtractMany(segments));
        self
    }

    // ── Spatial ──────────────────────────────────────────────────────

    /// Resize the video/image.
    #[must_use]
    pub fn resize(mut self, resolution: Resolution, mode: ResizeMode) -> Self {
        self.ops
            .push(MediaOp::Resize(ResizeOp { resolution, mode }));
        self
    }

    /// Crop the video/image.
    #[must_use]
    pub fn crop(mut self, region: CropRegion) -> Self {
        self.ops.push(MediaOp::Crop(region));
        self
    }

    /// Rotate the video/image.
    #[must_use]
    pub fn rotate(mut self, rotation: Rotation) -> Self {
        self.ops.push(MediaOp::Rotate(rotation));
        self
    }

    /// Flip the video/image.
    #[must_use]
    pub fn flip(mut self, direction: FlipDirection) -> Self {
        self.ops.push(MediaOp::Flip(direction));
        self
    }

    /// Pad the video/image.
    #[must_use]
    pub fn pad(mut self, width: u32, height: u32, color: &str) -> Self {
        self.ops.push(MediaOp::Pad(PadOp {
            width,
            height,
            color: color.to_string(),
        }));
        self
    }

    // ── Speed ────────────────────────────────────────────────────────

    /// Change playback speed.
    #[must_use]
    pub fn speed(mut self, factor: f64) -> Self {
        self.ops.push(MediaOp::Speed(factor));
        self
    }

    /// Reverse playback.
    #[must_use]
    pub fn reverse(mut self) -> Self {
        self.ops.push(MediaOp::Reverse);
        self
    }

    // ── Audio ────────────────────────────────────────────────────────

    /// Adjust volume.
    #[must_use]
    pub fn volume(mut self, factor: f64) -> Self {
        self.ops.push(MediaOp::Volume(factor));
        self
    }

    /// Normalize audio loudness.
    #[must_use]
    pub fn normalize_audio(mut self) -> Self {
        self.ops.push(MediaOp::NormalizeAudio);
        self
    }

    /// Fade in over the given duration.
    #[must_use]
    pub fn fade_in(mut self, duration: Duration) -> Self {
        self.ops.push(MediaOp::FadeIn(duration));
        self
    }

    /// Fade out over the given duration.
    #[must_use]
    pub fn fade_out(mut self, duration: Duration) -> Self {
        self.ops.push(MediaOp::FadeOut(duration));
        self
    }

    /// Remove the audio track.
    #[must_use]
    pub fn strip_audio(mut self) -> Self {
        self.ops.push(MediaOp::StripAudio);
        self
    }

    /// Remove the video track.
    #[must_use]
    pub fn strip_video(mut self) -> Self {
        self.ops.push(MediaOp::StripVideo);
        self
    }

    // ── Filters ──────────────────────────────────────────────────────

    /// Apply a filter.
    #[must_use]
    pub fn filter(mut self, filter: Filter) -> Self {
        self.ops.push(MediaOp::Filter(filter));
        self
    }

    // ── Composition ──────────────────────────────────────────────────

    /// Overlay another source.
    #[must_use]
    pub fn overlay(mut self, source: &FileSource, position: OverlayPosition, opacity: f32) -> Self {
        self.ops.push(MediaOp::Overlay(OverlayOp {
            source: source.clone(),
            position,
            opacity,
            time_range: None,
            scale: None,
        }));
        self
    }

    /// Concatenate another source.
    #[must_use]
    pub fn concat(mut self, source: &FileSource) -> Self {
        self.ops.push(MediaOp::Concat(ConcatOp {
            source: source.clone(),
            transition: None,
        }));
        self
    }

    /// Concatenate another source with a transition.
    #[must_use]
    pub fn concat_with_transition(mut self, source: &FileSource, transition: Transition) -> Self {
        self.ops.push(MediaOp::Concat(ConcatOp {
            source: source.clone(),
            transition: Some(transition),
        }));
        self
    }

    /// Replace the audio track.
    #[must_use]
    pub fn replace_audio(mut self, audio: &FileSource) -> Self {
        self.ops.push(MediaOp::ReplaceAudio(ReplaceAudioOp {
            audio_source: audio.clone(),
            offset: None,
        }));
        self
    }

    /// Mix another audio source.
    #[must_use]
    pub fn mix_audio(mut self, audio: &FileSource, volume: f64) -> Self {
        self.ops.push(MediaOp::MixAudio(MixAudioOp {
            audio_source: audio.clone(),
            volume,
            offset: None,
        }));
        self
    }

    /// Burn subtitles into the video.
    #[must_use]
    pub fn burn_subtitles(mut self, subs: SubtitleTrack) -> Self {
        self.ops.push(MediaOp::BurnSubtitles(subs));
        self
    }

    // ── Advanced filter / effects ────────────────────────────────────

    /// Apply a color grading or visual filter preset.
    #[must_use]
    pub fn apply_filter(mut self, config: FilterConfig) -> Self {
        self.ops.push(MediaOp::ApplyFilter(config));
        self
    }

    /// Add a text or image overlay.
    #[must_use]
    pub fn add_overlay(mut self, config: OverlayConfig) -> Self {
        self.ops.push(MediaOp::AddOverlay(config));
        self
    }

    /// Extract a single frame as a thumbnail image.
    #[must_use]
    pub fn generate_thumbnail(mut self, config: ThumbnailConfig) -> Self {
        self.ops.push(MediaOp::GenerateThumbnail(config));
        self
    }

    /// Detect scene boundaries.
    #[must_use]
    pub fn detect_scenes(mut self, config: SceneDetectConfig) -> Self {
        self.ops.push(MediaOp::DetectScenes(config));
        self
    }

    /// Burn subtitles from an external SRT/VTT/ASS source.
    #[must_use]
    pub fn add_subtitles(mut self, config: SubtitleConfig) -> Self {
        self.ops.push(MediaOp::AddSubtitles(config));
        self
    }

    /// AI upscale using Real-ESRGAN.
    #[must_use]
    pub fn upscale(mut self, config: UpscaleConfig) -> Self {
        self.ops.push(MediaOp::Upscale(config));
        self
    }

    /// AI frame interpolation using RIFE.
    #[must_use]
    pub fn interpolate(mut self, config: InterpolateConfig) -> Self {
        self.ops.push(MediaOp::Interpolate(config));
        self
    }

    // ── Track selection ──────────────────────────────────────────────

    /// Select specific tracks by index.
    #[must_use]
    pub fn select_tracks(mut self, indices: Vec<usize>) -> Self {
        self.ops.push(MediaOp::SelectTracks(indices));
        self
    }

    /// Select tracks by kind.
    #[must_use]
    pub fn select_tracks_by_kind(mut self, kinds: Vec<TrackKind>) -> Self {
        self.ops.push(MediaOp::SelectTracksByKind(kinds));
        self
    }

    // ── Output ───────────────────────────────────────────────────────

    /// Transcode to a different format/codec.
    #[must_use]
    pub fn transcode(mut self, config: OutputConfig) -> Self {
        self.ops.push(MediaOp::Transcode(config));
        self
    }

    /// Configure streaming output (HLS, DASH, or RTMP).
    ///
    /// This is a convenience wrapper around `transcode()` that configures
    /// the output for streaming.
    #[must_use]
    pub fn stream_to(mut self, config: OutputConfig) -> Self {
        self.ops.push(MediaOp::Transcode(config));
        self
    }

    /// Set the output destination.
    #[must_use]
    pub fn output_to(mut self, sink: FileSink) -> Self {
        self.sink = Some(sink);
        self
    }

    // ── Validation ───────────────────────────────────────────────────

    /// Validate the pipeline without executing it.
    ///
    /// Checks operation compatibility, ordering conflicts, and whether
    /// the executor supports all operations.
    pub fn validate(&self, executor: &dyn MediaExecutor) -> AppResult<()> {
        // Check that all ops are supported by the executor
        for op in &self.ops {
            if !executor.supports(op) {
                return Err(rskit_errors::AppError::new(
                    rskit_errors::ErrorCode::InvalidInput,
                    format!("executor does not support operation: {op:?}"),
                ));
            }
        }

        // Check for conflicting operations
        let mut has_strip_audio = false;
        let mut has_strip_video = false;

        for op in &self.ops {
            match op {
                MediaOp::StripAudio => has_strip_audio = true,
                MediaOp::StripVideo => has_strip_video = true,
                MediaOp::Volume(_)
                | MediaOp::NormalizeAudio
                | MediaOp::MixAudio(_)
                | MediaOp::ReplaceAudio(_)
                    if has_strip_audio =>
                {
                    return Err(rskit_errors::AppError::new(
                        rskit_errors::ErrorCode::InvalidInput,
                        "audio operation after StripAudio has no effect",
                    ));
                }
                MediaOp::Resize(_) | MediaOp::Crop(_) | MediaOp::Rotate(_) | MediaOp::Flip(_)
                    if has_strip_video =>
                {
                    return Err(rskit_errors::AppError::new(
                        rskit_errors::ErrorCode::InvalidInput,
                        "video operation after StripVideo has no effect",
                    ));
                }
                _ => {}
            }
        }

        Ok(())
    }

    // ── Execution ────────────────────────────────────────────────────

    /// Execute the pipeline using the given backend.
    pub async fn execute(self, executor: &dyn MediaExecutor) -> AppResult<FileSource> {
        executor
            .execute(&self.source, &self.ops, self.sink.as_ref())
            .await
    }

    /// Execute with progress reporting.
    pub async fn execute_with_progress(
        self,
        executor: &dyn MediaExecutor,
        on_progress: impl Fn(Progress) + Send + Sync + 'static,
    ) -> AppResult<FileSource> {
        executor
            .execute_with_progress(
                &self.source,
                &self.ops,
                self.sink.as_ref(),
                Box::new(on_progress),
            )
            .await
    }

    /// Execute with cancellation support and optional progress reporting.
    pub async fn execute_cancellable(
        self,
        executor: &dyn MediaExecutor,
        on_progress: Option<Box<dyn Fn(Progress) + Send + Sync>>,
        cancel: CancellationToken,
    ) -> AppResult<FileSource> {
        executor
            .execute_cancellable(
                &self.source,
                &self.ops,
                self.sink.as_ref(),
                on_progress,
                cancel,
            )
            .await
    }

    // ── Inspection ───────────────────────────────────────────────────

    /// Get the list of recorded operations.
    pub fn operations(&self) -> &[MediaOp] {
        &self.ops
    }

    /// Estimate output duration based on operations and source duration.
    pub fn estimated_duration(&self, source_duration: Duration) -> Duration {
        let mut duration = source_duration;
        for op in &self.ops {
            match op {
                MediaOp::Extract(range) => {
                    duration = range.duration();
                }
                MediaOp::ExtractMany(segments) => {
                    let total_us: u64 = segments
                        .iter()
                        .map(|s| s.range.duration().as_micros() as u64)
                        .sum();
                    duration = Duration::from_micros(total_us);
                }
                MediaOp::Concat(_) => {
                    // Each concat appends one source — duration unknown, estimate double
                    duration = duration.saturating_mul(2);
                }
                MediaOp::Speed(factor) if *factor > 0.0 => {
                    let us = (duration.as_micros() as f64 / factor) as u64;
                    duration = Duration::from_micros(us);
                }
                _ => {}
            }
        }
        duration
    }
}

/// Execution progress report.
#[derive(Debug, Clone)]
pub struct Progress {
    /// Current processing position.
    pub position: Option<Timestamp>,
    /// Total duration of the source.
    pub total: Option<Duration>,
    /// Completion percentage (0.0 – 100.0).
    pub percent: Option<f32>,
    /// Processing speed (e.g., 2.0 = 2× real-time).
    pub speed: Option<f64>,
    /// Size of the output so far.
    pub output_size: Option<u64>,
    /// Estimated time remaining.
    pub eta: Option<Duration>,
}
