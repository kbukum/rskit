//! Duration-aware timeout calculation for media operations.
//!
//! Instead of fixed timeouts that fail on long content, this module
//! calculates timeouts dynamically based on:
//! - Source media duration
//! - Operation type (stream copy is fast, transcoding is slow)
//! - Base timeout (minimum floor)
//!
//! # Formula
//!
//! ```text
//! timeout = base_timeout + (media_duration × multiplier)
//! ```
//!
//! # Example
//!
//! ```rust
//! use rskit_media::timeout::{OperationKind, TimeoutCalculator};
//! use std::time::Duration;
//!
//! let calc = TimeoutCalculator::default();
//!
//! // 10-minute video, stream copy: ~2 min timeout
//! let t = calc.calculate(Duration::from_secs(600), OperationKind::StreamCopy);
//! assert!(t.as_secs() >= 90 && t.as_secs() <= 180);
//!
//! // 100-minute video, transcode: ~260 min timeout
//! let t = calc.calculate(Duration::from_secs(6000), OperationKind::Transcode);
//! assert!(t.as_secs() >= 12000);
//! ```

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Categories of media operations with different time complexity characteristics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum OperationKind {
    /// Stream copy (mux/demux only, no re-encoding).
    /// Very fast — typically faster than real-time.
    StreamCopy,
    /// Audio extraction or audio-only processing.
    /// Fast — audio is small relative to video.
    AudioProcess,
    /// Video transcoding (re-encoding).
    /// Slow — depends on codec, resolution, and complexity.
    Transcode,
    /// Video filter application (resize, crop, effects).
    /// Similar to transcoding but may be faster for simple filters.
    Filter,
    /// Subtitle burn-in (requires video re-encoding).
    /// Similar speed to transcoding.
    SubtitleBurn,
    /// Thumbnail/frame extraction.
    /// Very fast — single frame seeks.
    ThumbnailExtract,
    /// Scene detection (analyzing every frame).
    /// Medium — faster than transcoding but needs to decode all frames.
    SceneDetect,
    /// ML inference (transcription, sentiment, etc.).
    /// Speed varies widely; use a generous multiplier.
    MlInference,
    /// Media probe (ffprobe).
    /// Nearly instant — fixed small timeout.
    Probe,
}

impl OperationKind {
    /// Default timeout multiplier for this operation kind.
    ///
    /// The multiplier is applied to the media duration to compute
    /// the variable portion of the timeout.
    pub fn default_multiplier(&self) -> f64 {
        match self {
            Self::Probe => 0.0,            // Fixed timeout only
            Self::StreamCopy => 0.2,       // 10 min video → ~2 min
            Self::ThumbnailExtract => 0.1, // 10 min video → ~1 min
            Self::AudioProcess => 0.5,     // 10 min video → ~5 min
            Self::SceneDetect => 1.5,      // 10 min video → ~15 min
            Self::Filter => 2.0,           // 10 min video → ~20 min
            Self::Transcode => 2.5,        // 10 min video → ~25 min
            Self::SubtitleBurn => 3.0,     // 10 min video → ~30 min
            Self::MlInference => 5.0,      // 10 min video → ~50 min
        }
    }

    /// Default base timeout for this operation kind.
    pub fn default_base_timeout(&self) -> Duration {
        match self {
            Self::Probe => Duration::from_secs(30),
            Self::ThumbnailExtract => Duration::from_secs(30),
            Self::StreamCopy => Duration::from_secs(60),
            Self::AudioProcess => Duration::from_secs(60),
            Self::SceneDetect => Duration::from_secs(120),
            Self::Filter => Duration::from_secs(120),
            Self::Transcode => Duration::from_secs(120),
            Self::SubtitleBurn => Duration::from_secs(120),
            Self::MlInference => Duration::from_secs(300),
        }
    }
}

/// Duration-aware timeout calculator.
///
/// Computes timeouts dynamically based on media duration and operation type.
/// Replaces fixed timeout values that fail on long content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutCalculator {
    /// Base timeout — minimum floor regardless of duration.
    pub base_timeout: Duration,
    /// Maximum timeout — ceiling to prevent runaway processes.
    pub max_timeout: Duration,
    /// Custom multiplier overrides per operation kind.
    /// If not set, uses [`OperationKind::default_multiplier`].
    #[serde(default)]
    pub multiplier_overrides: Vec<(OperationKind, f64)>,
}

impl Default for TimeoutCalculator {
    fn default() -> Self {
        Self {
            base_timeout: Duration::from_secs(60),
            max_timeout: Duration::from_secs(4 * 3600), // 4 hours absolute max
            multiplier_overrides: Vec::new(),
        }
    }
}

impl TimeoutCalculator {
    /// Create with a custom base timeout.
    #[must_use]
    pub fn with_base_timeout(mut self, base: Duration) -> Self {
        self.base_timeout = base;
        self
    }

    /// Set the maximum timeout ceiling.
    #[must_use]
    pub fn with_max_timeout(mut self, max: Duration) -> Self {
        self.max_timeout = max;
        self
    }

    /// Override the multiplier for a specific operation kind.
    #[must_use]
    pub fn with_multiplier(mut self, kind: OperationKind, multiplier: f64) -> Self {
        // Remove existing override if present
        self.multiplier_overrides.retain(|(k, _)| *k != kind);
        self.multiplier_overrides.push((kind, multiplier));
        self
    }

    /// Calculate timeout for a given media duration and operation.
    pub fn calculate(&self, media_duration: Duration, operation: OperationKind) -> Duration {
        let multiplier = self
            .multiplier_overrides
            .iter()
            .find(|(k, _)| *k == operation)
            .map(|(_, m)| *m)
            .unwrap_or_else(|| operation.default_multiplier());

        let base = self.base_timeout.max(operation.default_base_timeout());
        let variable = Duration::from_secs_f64(media_duration.as_secs_f64() * multiplier);
        let total = base + variable;

        // Clamp to max
        total.min(self.max_timeout)
    }

    /// Calculate timeout for a chunk (uses chunk duration, not total duration).
    pub fn calculate_for_chunk(
        &self,
        chunk_duration: Duration,
        operation: OperationKind,
    ) -> Duration {
        self.calculate(chunk_duration, operation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_probe_timeout_is_small() {
        let calc = TimeoutCalculator::default();
        let t = calc.calculate(Duration::from_secs(6000), OperationKind::Probe);
        // Probe: base = max(60, 30) = 60s, multiplier = 0.0 → 60s
        assert_eq!(t.as_secs(), 60);
    }

    #[test]
    fn stream_copy_is_fast() {
        let calc = TimeoutCalculator::default();
        // 60 min video, stream copy
        let t = calc.calculate(Duration::from_secs(3600), OperationKind::StreamCopy);
        // base(60) + 3600 * 0.2 = 60 + 720 = 780s = 13 min
        assert_eq!(t.as_secs(), 780);
    }

    #[test]
    fn transcode_scales_with_duration() {
        let calc = TimeoutCalculator::default();
        // 100 min video, transcode
        let t = calc.calculate(Duration::from_secs(6000), OperationKind::Transcode);
        // base(120) + 6000 * 2.5 = 120 + 15000 = 15120s = ~4.2 hours → clamped to 4hr
        assert_eq!(t, Duration::from_secs(4 * 3600));
    }

    #[test]
    fn ml_inference_is_generous() {
        let calc = TimeoutCalculator::default();
        // 10 min audio, transcription
        let t = calc.calculate(Duration::from_secs(600), OperationKind::MlInference);
        // base(300) + 600 * 5.0 = 300 + 3000 = 3300s = 55 min
        assert_eq!(t.as_secs(), 3300);
    }

    #[test]
    fn custom_multiplier_override() {
        let calc = TimeoutCalculator::default().with_multiplier(OperationKind::Transcode, 1.0);
        // 60 min video with custom 1.0x multiplier
        let t = calc.calculate(Duration::from_secs(3600), OperationKind::Transcode);
        // base(120) + 3600 * 1.0 = 3720s
        assert_eq!(t.as_secs(), 3720);
    }

    #[test]
    fn max_timeout_is_respected() {
        let calc = TimeoutCalculator::default().with_max_timeout(Duration::from_secs(600));
        let t = calc.calculate(Duration::from_secs(3600), OperationKind::Transcode);
        assert_eq!(t.as_secs(), 600);
    }

    #[test]
    fn chunk_timeout_uses_chunk_duration() {
        let calc = TimeoutCalculator::default();
        // 10 min chunk of a 100 min video
        let chunk_t = calc.calculate_for_chunk(Duration::from_secs(600), OperationKind::Transcode);
        let full_t = calc.calculate(Duration::from_secs(6000), OperationKind::Transcode);
        assert!(chunk_t < full_t);
    }
}
