//! Duration-aware timeout resolution and source introspection.
//!
//! Provides helpers used by [`FfmpegExecutor`] before executing commands:
//! - [`infer_operation_kind`] — classify a `MediaOp` list into an [`OperationKind`]
//! - [`FfmpegExecutor::resolve_effective_config`] — probe duration + compute timeout
//! - [`FfmpegExecutor::quick_probe_duration`] — lightweight ffprobe for duration only
//! - [`FfmpegExecutor::build_source_hints`] — detect audio/video streams for hints

use std::time::Duration;

use rskit_storage::FileSource;
use rskit_media::ops::MediaOp;
use rskit_media::timeout::OperationKind;

use crate::command::SourceHints;
use crate::config::FfmpegConfig;

use super::FfmpegExecutor;

/// Infer the dominant [`OperationKind`] from a list of media operations.
///
/// Picks the "heaviest" operation kind when multiple ops are present,
/// since the total timeout should be driven by the most expensive step.
#[must_use]
pub(crate) fn infer_operation_kind(ops: &[MediaOp]) -> OperationKind {
    let mut heaviest = OperationKind::StreamCopy;

    for op in ops {
        let kind = match op {
            // Temporal / track selection — typically stream copy, fast
            MediaOp::Extract(_)
            | MediaOp::StripAudio
            | MediaOp::StripVideo
            | MediaOp::SelectTracks(_)
            | MediaOp::SelectTracksByKind(_) => OperationKind::StreamCopy,

            // Single frame extraction
            MediaOp::GenerateThumbnail(_) => OperationKind::ThumbnailExtract,

            // Multi-segment extraction (may need concat)
            MediaOp::ExtractMany(_) => OperationKind::Filter,

            // Scene detection
            MediaOp::DetectScenes(_) => OperationKind::SceneDetect,

            // Full transcode / concat
            MediaOp::Resize(_) | MediaOp::Transcode(_) | MediaOp::Concat(_) => {
                OperationKind::Transcode
            }

            // Video/audio filters
            MediaOp::Crop(_)
            | MediaOp::Rotate(_)
            | MediaOp::Flip(_)
            | MediaOp::Pad(_)
            | MediaOp::Speed(_)
            | MediaOp::Reverse
            | MediaOp::Volume(_)
            | MediaOp::NormalizeAudio
            | MediaOp::FadeIn(_)
            | MediaOp::FadeOut(_)
            | MediaOp::Filter(_)
            | MediaOp::Overlay(_)
            | MediaOp::ReplaceAudio(_)
            | MediaOp::MixAudio(_)
            | MediaOp::ApplyFilter(_)
            | MediaOp::AddOverlay(_) => OperationKind::Filter,

            // Subtitle burn-in
            MediaOp::BurnSubtitles(_) | MediaOp::AddSubtitles(_) => OperationKind::SubtitleBurn,

            // AI-powered operations (external tools)
            MediaOp::Upscale(_) | MediaOp::Interpolate(_) => OperationKind::MlInference,

            // Unknown future variants — conservative default
            _ => OperationKind::Transcode,
        };

        if kind.default_multiplier() > heaviest.default_multiplier() {
            heaviest = kind;
        }
    }

    heaviest
}

impl FfmpegExecutor {
    /// Create an effective config with duration-aware timeout resolved.
    ///
    /// When the config has a [`rskit_media::timeout::TimeoutCalculator`], this probes the source
    /// duration and infers the operation kind to compute a scaled timeout,
    /// replacing the fixed `timeout` field in the returned config.
    pub(crate) async fn resolve_effective_config(
        &self,
        source: &FileSource,
        ops: &[MediaOp],
    ) -> FfmpegConfig {
        // If no calculator is configured, skip the probe entirely.
        if self.config.timeout_calculator.is_none() {
            return self.config.clone();
        }

        let source_duration = self.quick_probe_duration(source).await;
        let op_kind = infer_operation_kind(ops);

        if let Some(resolved) = self.config.resolve_timeout(source_duration, Some(op_kind)) {
            tracing::debug!(
                source_duration_secs = source_duration.map(|d| d.as_secs()),
                op_kind = ?op_kind,
                resolved_timeout_secs = resolved.as_secs(),
                "resolved duration-aware timeout"
            );
            let mut cfg = self.config.clone();
            cfg.timeout = Some(resolved);
            cfg
        } else {
            self.config.clone()
        }
    }

    /// Quick ffprobe to get source duration (for timeout calculation).
    pub(crate) async fn quick_probe_duration(&self, source: &FileSource) -> Option<Duration> {
        let path = match source {
            FileSource::Path(p) => p.clone(),
            _ => return None,
        };

        let output = tokio::process::Command::new(self.config.ffprobe_bin())
            .args([
                "-v",
                "quiet",
                "-show_entries",
                "format=duration",
                "-of",
                "csv=p=0",
            ])
            .arg(&path)
            .output()
            .await
            .ok()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let secs: f64 = stdout.trim().parse().ok()?;
        Some(Duration::from_secs_f64(secs))
    }

    /// Build source hints by quick-probing when concat/extract-many ops need stream info.
    pub(crate) async fn build_source_hints(
        &self,
        source: &FileSource,
        ops: &[MediaOp],
    ) -> SourceHints {
        let needs_hints = ops
            .iter()
            .any(|op| matches!(op, MediaOp::ExtractMany(_) | MediaOp::Concat(_)));
        if !needs_hints {
            return SourceHints::default();
        }

        // Quick ffprobe to detect stream types
        let path = match source {
            FileSource::Path(p) => p.clone(),
            _ => return SourceHints::default(),
        };

        let output = tokio::process::Command::new(self.config.ffprobe_bin())
            .args([
                "-v",
                "quiet",
                "-show_entries",
                "stream=codec_type",
                "-of",
                "csv=p=0",
            ])
            .arg(&path)
            .output()
            .await;

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let has_audio = stdout.lines().any(|l| l.trim() == "audio");
                let has_video = stdout.lines().any(|l| l.trim() == "video");
                SourceHints {
                    has_audio: Some(has_audio),
                    has_video: Some(has_video),
                }
            }
            Err(_) => SourceHints::default(),
        }
    }
}
