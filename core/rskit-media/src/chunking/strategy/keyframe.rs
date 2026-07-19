//! Keyframe-aligned chunk strategy.

use std::time::Duration;

use rskit_errors::AppResult;

use crate::chunking::types::{ChunkBoundary, ChunkId, ChunkPlan, ChunkedOperation, ReassemblyPlan};
use crate::probe::MediaMetadata;
use crate::time::{TimeRange, Timestamp};

use super::chunk_strategy::ChunkStrategy;
use super::fixed_duration::FixedDurationStrategy;

/// Split media at keyframe boundaries (for lossless video splitting).
///
/// Attempts to place chunk boundaries on keyframes to avoid re-encoding at split points.
pub struct KeyframeStrategy {
    /// Target chunk count (will adjust based on available keyframes).
    pub target_chunks: usize,
    /// Minimum chunk duration (avoids tiny chunks).
    pub min_chunk_duration: Duration,
    /// Timeout multiplier per chunk.
    pub timeout_multiplier: f64,
}

impl Default for KeyframeStrategy {
    fn default() -> Self {
        Self {
            target_chunks: 4,
            min_chunk_duration: Duration::from_secs(60),
            timeout_multiplier: 3.0,
        }
    }
}

impl KeyframeStrategy {
    /// Set the target number of chunks.
    #[must_use]
    pub fn with_target_chunks(mut self, count: usize) -> Self {
        self.target_chunks = count.max(1);
        self
    }
}

impl ChunkStrategy for KeyframeStrategy {
    fn name(&self) -> &str {
        "keyframe"
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

        // Filter to keyframe-only boundaries
        let keyframes: Vec<&ChunkBoundary> = boundaries.iter().filter(|b| b.is_keyframe).collect();

        // If too few keyframes, fall back to fixed duration
        if keyframes.len() < self.target_chunks {
            let fallback = FixedDurationStrategy {
                chunk_duration: Duration::from_secs_f64(
                    total_duration.as_secs_f64() / self.target_chunks as f64,
                ),
                timeout_multiplier: self.timeout_multiplier,
                ..Default::default()
            };
            return fallback.plan(metadata, boundaries);
        }

        // Select keyframes that divide the media into roughly equal parts
        let ideal_chunk_duration = total_duration.as_secs_f64() / self.target_chunks as f64;
        let min_chunk_us = self.min_chunk_duration.as_micros() as u64;
        let total_us = total_duration.as_micros() as u64;

        let mut split_points: Vec<Timestamp> = Vec::new();
        let mut next_ideal_us = (ideal_chunk_duration * 1_000_000.0) as u64;

        for kf in &keyframes {
            if kf.timestamp.as_micros() >= total_us {
                break;
            }
            if kf.timestamp.as_micros() >= next_ideal_us
                && kf.timestamp.as_micros() >= min_chunk_us
                && (total_us - kf.timestamp.as_micros()) >= min_chunk_us
            {
                split_points.push(kf.timestamp);
                next_ideal_us =
                    kf.timestamp.as_micros() + (ideal_chunk_duration * 1_000_000.0) as u64;
            }
        }

        // Build chunk plans from split points
        let mut chunks = Vec::new();
        let mut current_start = Timestamp(0);

        for (index, &split) in split_points.iter().enumerate() {
            let range = TimeRange::new(current_start, split);
            let chunk_dur = range.duration();
            chunks.push(ChunkPlan {
                id: ChunkId::from_index(index),
                index,
                range,
                start_is_keyframe: true,
                suggested_timeout: Duration::from_secs_f64(
                    chunk_dur.as_secs_f64() * self.timeout_multiplier,
                ),
            });
            current_start = split;
        }

        // Final chunk
        let final_range = TimeRange::new(
            current_start,
            Timestamp::from_seconds(total_duration.as_secs_f64()),
        );
        let final_dur = final_range.duration();
        chunks.push(ChunkPlan {
            id: ChunkId::from_index(split_points.len()),
            index: split_points.len(),
            range: final_range,
            start_is_keyframe: true,
            suggested_timeout: Duration::from_secs_f64(
                final_dur.as_secs_f64() * self.timeout_multiplier,
            ),
        });

        Ok(ChunkedOperation {
            chunks,
            reassembly: ReassemblyPlan::Concat,
            total_duration,
            strategy_name: self.name().to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{make_keyframe_boundaries, make_metadata};
    use super::*;

    #[test]
    fn keyframe_strategy_creates_target_chunks() {
        let strategy = KeyframeStrategy {
            target_chunks: 4,
            ..Default::default()
        };
        let metadata = make_metadata(3600.0);
        let boundaries = make_keyframe_boundaries(3600.0, 2.0);
        let result = strategy.plan(&metadata, &boundaries).unwrap();
        // Should create roughly 4-5 chunks (exact count depends on keyframe alignment)
        assert!(result.chunk_count() >= 3);
        assert!(result.chunk_count() <= 6);
    }

    #[test]
    fn keyframe_strategy_handles_zero_duration_and_fallback() {
        let zero = make_metadata(0.0);
        assert!(KeyframeStrategy::default().plan(&zero, &[]).is_err());

        let strategy = KeyframeStrategy::default().with_target_chunks(0);
        assert_eq!(strategy.target_chunks, 1);

        let metadata = make_metadata(1200.0);
        let sparse = vec![ChunkBoundary {
            timestamp: Timestamp::from_seconds(60.0),
            is_keyframe: true,
            quality: 1.0,
        }];
        let plan = KeyframeStrategy {
            target_chunks: 4,
            ..Default::default()
        }
        .plan(&metadata, &sparse)
        .unwrap();
        assert_eq!(plan.strategy_name, "fixed_duration");
    }
}
