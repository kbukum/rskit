//! Silence-aligned chunk strategy for transcription.

use std::time::Duration;

use rskit_errors::AppResult;

use crate::chunking::types::{ChunkBoundary, ChunkId, ChunkPlan, ChunkedOperation, ReassemblyPlan};
use crate::probe::MediaMetadata;
use crate::time::{TimeRange, Timestamp};

use super::chunk_strategy::ChunkStrategy;

/// Split audio at silence boundaries (for transcription chunking).
///
/// Uses silence detection points as chunk boundaries to produce natural-sounding splits without cutting mid-sentence.
pub struct SilenceStrategy {
    /// Target chunk duration.
    pub target_chunk_duration: Duration,
    /// Maximum allowed deviation from target when picking silence boundaries.
    pub max_deviation: Duration,
    /// Timeout multiplier per chunk.
    pub timeout_multiplier: f64,
}

impl Default for SilenceStrategy {
    fn default() -> Self {
        Self {
            target_chunk_duration: Duration::from_secs(600), // 10 minutes
            max_deviation: Duration::from_secs(60),          // ±1 minute
            timeout_multiplier: 5.0, // Transcription is slower than real-time
        }
    }
}

impl SilenceStrategy {
    /// Set the target chunk duration.
    #[must_use]
    pub fn with_target_duration(mut self, duration: Duration) -> Self {
        self.target_chunk_duration = duration;
        self
    }
}

impl ChunkStrategy for SilenceStrategy {
    fn name(&self) -> &str {
        "silence"
    }

    fn plan(
        &self,
        metadata: &MediaMetadata,
        boundaries: &[ChunkBoundary],
    ) -> AppResult<ChunkedOperation> {
        let total_duration = metadata.duration.unwrap_or_default();
        if total_duration.is_zero() {
            return Err(rskit_errors::AppError::new(
                rskit_errors::ErrorCode::InvalidInput,
                "cannot chunk media with zero duration",
            ));
        }
        if self.target_chunk_duration.is_zero() {
            return Err(rskit_errors::AppError::new(
                rskit_errors::ErrorCode::InvalidInput,
                "target_chunk_duration must be greater than zero",
            ));
        }

        let total_us = total_duration.as_micros() as u64;
        let target_us = self.target_chunk_duration.as_micros() as u64;
        let deviation_us = self.max_deviation.as_micros() as u64;

        // Filter to high-quality (silence) boundaries
        let silence_points: Vec<&ChunkBoundary> =
            boundaries.iter().filter(|b| b.quality >= 0.5).collect();

        let mut chunks = Vec::new();
        let mut current_start = Timestamp(0);
        let mut index = 0;

        while current_start.as_micros() < total_us {
            let ideal_end_us = current_start
                .as_micros()
                .saturating_add(target_us)
                .min(total_us);
            let remaining = total_us.saturating_sub(ideal_end_us);

            // If remaining is less than half a chunk, extend to end
            let end = if remaining < target_us / 2 {
                Timestamp(total_us)
            } else {
                // Find closest silence point within tolerance
                let min_us = ideal_end_us.saturating_sub(deviation_us);
                let max_us = ideal_end_us.saturating_add(deviation_us).min(total_us);

                let best = silence_points
                    .iter()
                    .filter(|b| {
                        b.timestamp.as_micros() >= min_us && b.timestamp.as_micros() <= max_us
                    })
                    .min_by_key(|b| {
                        (b.timestamp.as_micros() as i64 - ideal_end_us as i64).unsigned_abs()
                    });

                match best {
                    Some(b) => b.timestamp,
                    // No silence point found — use ideal end
                    None => Timestamp(ideal_end_us),
                }
            };

            let range = TimeRange::new(current_start, end);
            let chunk_dur = range.duration();

            chunks.push(ChunkPlan {
                id: ChunkId::from_index(index),
                index,
                range,
                start_is_keyframe: false, // Audio doesn't have keyframes
                suggested_timeout: Duration::from_secs_f64(
                    chunk_dur.as_secs_f64() * self.timeout_multiplier,
                ),
            });

            current_start = end;
            index += 1;
        }

        Ok(ChunkedOperation {
            chunks,
            reassembly: ReassemblyPlan::MergeText {
                separator: String::new(),
            },
            total_duration,
            strategy_name: self.name().to_string(),
        })
    }

    fn min_duration(&self) -> Option<Duration> {
        Some(self.target_chunk_duration.mul_f64(1.5))
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::make_metadata;
    use super::*;

    #[test]
    fn silence_strategy_plans_for_transcription() {
        let strategy = SilenceStrategy::default(); // 10 min chunks
        let metadata = make_metadata(3600.0); // 60 minutes

        // Create silence points at ~10 min intervals with some jitter
        let silence_points = vec![
            ChunkBoundary {
                timestamp: Timestamp::from_seconds(590.0),
                is_keyframe: false,
                quality: 0.8,
            },
            ChunkBoundary {
                timestamp: Timestamp::from_seconds(1210.0),
                is_keyframe: false,
                quality: 0.9,
            },
            ChunkBoundary {
                timestamp: Timestamp::from_seconds(1800.0),
                is_keyframe: false,
                quality: 0.7,
            },
            ChunkBoundary {
                timestamp: Timestamp::from_seconds(2405.0),
                is_keyframe: false,
                quality: 0.85,
            },
            ChunkBoundary {
                timestamp: Timestamp::from_seconds(3010.0),
                is_keyframe: false,
                quality: 0.9,
            },
        ];

        let result = strategy.plan(&metadata, &silence_points).unwrap();
        assert!(result.chunk_count() >= 5);
        assert!(matches!(
            result.reassembly,
            ReassemblyPlan::MergeText { .. }
        ));
    }

    #[test]
    fn silence_strategy_builders_zero_duration_and_no_boundary_fallback() {
        let strategy = SilenceStrategy::default().with_target_duration(Duration::from_secs(120));
        assert_eq!(strategy.min_duration(), Some(Duration::from_secs(180)));
        assert!(strategy.plan(&make_metadata(0.0), &[]).is_err());

        let plan = strategy.plan(&make_metadata(600.0), &[]).unwrap();
        assert!(plan.chunk_count() > 1);
        assert_eq!(plan.chunks[0].range.end, Timestamp::from_seconds(120.0));
    }

    #[test]
    fn silence_strategy_rejects_zero_target_duration() {
        let strategy = SilenceStrategy::default().with_target_duration(Duration::ZERO);
        let err = strategy.plan(&make_metadata(600.0), &[]).unwrap_err();
        assert_eq!(err.code(), rskit_errors::ErrorCode::InvalidInput);
    }
}
