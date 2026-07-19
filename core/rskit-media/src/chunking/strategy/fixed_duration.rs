//! Fixed-duration chunk strategy.

use std::time::Duration;

use rskit_errors::AppResult;

use crate::chunking::types::{ChunkBoundary, ChunkId, ChunkPlan, ChunkedOperation, ReassemblyPlan};
use crate::probe::MediaMetadata;
use crate::time::{TimeRange, Timestamp};

use super::boundary::snap_to_boundary;
use super::chunk_strategy::ChunkStrategy;

/// Split media at fixed duration intervals.
///
/// Simple strategy that creates equal-sized chunks.
/// Chunk boundaries are snapped to the nearest provided boundary point (keyframe/silence).
pub struct FixedDurationStrategy {
    /// Target duration per chunk.
    pub chunk_duration: Duration,
    /// Maximum allowed deviation from target when snapping to boundaries.
    pub snap_tolerance: Duration,
    /// How to reassemble chunks.
    pub reassembly: ReassemblyPlan,
    /// Timeout multiplier per chunk (based on chunk duration).
    pub timeout_multiplier: f64,
}

impl Default for FixedDurationStrategy {
    fn default() -> Self {
        Self {
            chunk_duration: Duration::from_secs(600), // 10 minutes
            snap_tolerance: Duration::from_secs(5),
            reassembly: ReassemblyPlan::Concat,
            timeout_multiplier: 3.0,
        }
    }
}

impl FixedDurationStrategy {
    /// Create a strategy with the given chunk duration.
    #[must_use]
    pub fn with_chunk_duration(mut self, duration: Duration) -> Self {
        self.chunk_duration = duration;
        self
    }

    /// Set the snap tolerance.
    #[must_use]
    pub fn with_snap_tolerance(mut self, tolerance: Duration) -> Self {
        self.snap_tolerance = tolerance;
        self
    }

    /// Set the reassembly plan.
    #[must_use]
    pub fn with_reassembly(mut self, reassembly: ReassemblyPlan) -> Self {
        self.reassembly = reassembly;
        self
    }

    /// Set the timeout multiplier.
    #[must_use]
    pub fn with_timeout_multiplier(mut self, multiplier: f64) -> Self {
        self.timeout_multiplier = multiplier;
        self
    }
}

impl ChunkStrategy for FixedDurationStrategy {
    fn name(&self) -> &str {
        "fixed_duration"
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

        // If duration is less than 1.5x chunk size, don't bother splitting
        let threshold = self.chunk_duration.mul_f64(1.5);
        if total_duration <= threshold {
            let plan = ChunkPlan {
                id: ChunkId::from_index(0),
                index: 0,
                range: TimeRange::new(
                    Timestamp(0),
                    Timestamp::from_seconds(total_duration.as_secs_f64()),
                ),
                start_is_keyframe: true,
                suggested_timeout: Duration::from_secs_f64(
                    total_duration.as_secs_f64() * self.timeout_multiplier,
                ),
            };
            return Ok(ChunkedOperation {
                chunks: vec![plan],
                reassembly: self.reassembly.clone(),
                total_duration,
                strategy_name: self.name().to_string(),
            });
        }

        let chunk_us = self.chunk_duration.as_micros() as u64;
        let total_us = total_duration.as_micros() as u64;
        let snap_us = self.snap_tolerance.as_micros() as u64;

        let mut chunks = Vec::new();
        let mut current_start = Timestamp(0);
        let mut index = 0;

        while current_start.as_micros() < total_us {
            let ideal_end_us = current_start
                .as_micros()
                .saturating_add(chunk_us)
                .min(total_us);
            let ideal_end = Timestamp(ideal_end_us);

            // If this is the last chunk (or close to the end), extend to the end
            let remaining = total_us.saturating_sub(ideal_end_us);
            let end = if remaining < chunk_us / 2 {
                Timestamp(total_us)
            } else {
                snap_to_boundary(ideal_end, boundaries, snap_us)
            };

            let is_keyframe = if index == 0 {
                true
            } else {
                boundaries
                    .iter()
                    .any(|b| b.is_keyframe && b.timestamp == current_start)
            };

            let range = TimeRange::new(current_start, end);
            let chunk_dur = range.duration();

            chunks.push(ChunkPlan {
                id: ChunkId::from_index(index),
                index,
                range,
                start_is_keyframe: is_keyframe,
                suggested_timeout: Duration::from_secs_f64(
                    chunk_dur.as_secs_f64() * self.timeout_multiplier,
                ),
            });

            current_start = end;
            index += 1;
        }

        Ok(ChunkedOperation {
            chunks,
            reassembly: self.reassembly.clone(),
            total_duration,
            strategy_name: self.name().to_string(),
        })
    }

    fn min_duration(&self) -> Option<Duration> {
        Some(self.chunk_duration.mul_f64(1.5))
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{make_keyframe_boundaries, make_metadata};
    use super::*;

    #[test]
    fn fixed_duration_single_chunk_for_short_media() {
        let strategy = FixedDurationStrategy::default(); // 10 min chunks
        let metadata = make_metadata(300.0); // 5 minutes
        let result = strategy.plan(&metadata, &[]).unwrap();
        assert_eq!(result.chunk_count(), 1);
        assert!(result.is_single_chunk());
    }

    #[test]
    fn fixed_duration_multiple_chunks_for_long_media() {
        let strategy = FixedDurationStrategy::default(); // 10 min chunks
        let metadata = make_metadata(3600.0); // 60 minutes
        let boundaries = make_keyframe_boundaries(3600.0, 2.0);
        let result = strategy.plan(&metadata, &boundaries).unwrap();
        assert!(result.chunk_count() >= 5);
        assert!(result.chunk_count() <= 7);

        // Verify chunks cover the full duration
        let first = &result.chunks[0];
        let last = result.chunks.last().unwrap();
        assert_eq!(first.range.start.as_micros(), 0);
        assert_eq!(last.range.end.as_millis(), 3_600_000);
    }

    #[test]
    fn fixed_duration_rejects_zero_duration() {
        let strategy = FixedDurationStrategy::default();
        let metadata = make_metadata(0.0);
        assert!(strategy.plan(&metadata, &[]).is_err());
    }

    #[test]
    fn fixed_duration_builders_and_min_duration_are_reported() {
        let strategy = FixedDurationStrategy::default()
            .with_chunk_duration(Duration::from_secs(120))
            .with_snap_tolerance(Duration::from_secs(3))
            .with_reassembly(ReassemblyPlan::MergeText {
                separator: "\n".to_string(),
            })
            .with_timeout_multiplier(2.0);

        assert_eq!(strategy.min_duration(), Some(Duration::from_secs(180)));
        assert_eq!(strategy.snap_tolerance, Duration::from_secs(3));
        assert_eq!(strategy.timeout_multiplier, 2.0);
    }
}
