//! Chunking strategy traits and built-in implementations.

use std::time::Duration;

use rskit_errors::AppResult;

use crate::probe::MediaMetadata;
use crate::time::{TimeRange, Timestamp};

use super::types::{ChunkBoundary, ChunkId, ChunkPlan, ChunkedOperation, ReassemblyPlan};

/// Strategy for splitting media into processable chunks.
///
/// Implementations determine where to place chunk boundaries based on the media metadata
/// and operation requirements.
pub trait ChunkStrategy: Send + Sync {
    /// Human-readable name of this strategy.
    fn name(&self) -> &str;

    /// Plan chunk boundaries for the given media.
    ///
    /// Returns a complete [`ChunkedOperation`] with ordered chunk plans and reassembly instructions.
    fn plan(
        &self,
        metadata: &MediaMetadata,
        boundaries: &[ChunkBoundary],
    ) -> AppResult<ChunkedOperation>;

    /// Minimum duration below which chunking is not worthwhile.
    /// Returns `None` if this strategy always chunks.
    fn min_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs(600)) // 10 minutes default
    }
}

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

/// Snap a timestamp to the nearest boundary within tolerance.
fn snap_to_boundary(
    ideal: Timestamp,
    boundaries: &[ChunkBoundary],
    tolerance_us: u64,
) -> Timestamp {
    if boundaries.is_empty() {
        return ideal;
    }

    let ideal_us = ideal.as_micros();
    let min_us = ideal_us.saturating_sub(tolerance_us);
    let max_us = ideal_us.saturating_add(tolerance_us);

    // Prefer keyframe boundaries, then closest
    let best_keyframe = boundaries
        .iter()
        .filter(|b| {
            b.is_keyframe && b.timestamp.as_micros() >= min_us && b.timestamp.as_micros() <= max_us
        })
        .min_by_key(|b| (b.timestamp.as_micros() as i64 - ideal_us as i64).unsigned_abs());

    if let Some(kf) = best_keyframe {
        return kf.timestamp;
    }

    // Fall back to any boundary
    let best_any = boundaries
        .iter()
        .filter(|b| b.timestamp.as_micros() >= min_us && b.timestamp.as_micros() <= max_us)
        .min_by_key(|b| (b.timestamp.as_micros() as i64 - ideal_us as i64).unsigned_abs());

    best_any.map_or(ideal, |b| b.timestamp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::Format;
    use crate::types::MediaType;

    fn make_metadata(duration_secs: f64) -> MediaMetadata {
        MediaMetadata {
            media_type: MediaType::Video,
            format: Format::new("mp4"),
            duration: Some(Duration::from_secs_f64(duration_secs)),
            size: None,
            bitrate: None,
            tracks: vec![],
            tags: Default::default(),
            created_at: None,
        }
    }

    fn make_keyframe_boundaries(duration_secs: f64, interval_secs: f64) -> Vec<ChunkBoundary> {
        let mut boundaries = Vec::new();
        let mut t = 0.0;
        while t < duration_secs {
            boundaries.push(ChunkBoundary {
                timestamp: Timestamp::from_seconds(t),
                is_keyframe: true,
                quality: 1.0,
            });
            t += interval_secs;
        }
        boundaries
    }

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
    fn snap_to_boundary_prefers_keyframe() {
        let boundaries = vec![
            ChunkBoundary {
                timestamp: Timestamp::from_seconds(9.5),
                is_keyframe: false,
                quality: 0.8,
            },
            ChunkBoundary {
                timestamp: Timestamp::from_seconds(10.5),
                is_keyframe: true,
                quality: 1.0,
            },
            ChunkBoundary {
                timestamp: Timestamp::from_seconds(11.0),
                is_keyframe: false,
                quality: 0.5,
            },
        ];
        let result = snap_to_boundary(
            Timestamp::from_seconds(10.0),
            &boundaries,
            2_000_000, // 2 second tolerance
        );
        assert_eq!(result.as_millis(), 10500); // Prefers the keyframe
    }

    #[test]
    fn snap_to_boundary_handles_empty_and_any_boundary_fallback() {
        let ideal = Timestamp::from_seconds(10.0);
        assert_eq!(snap_to_boundary(ideal, &[], 1_000_000), ideal);

        let boundaries = [ChunkBoundary {
            timestamp: Timestamp::from_seconds(9.8),
            is_keyframe: false,
            quality: 0.7,
        }];
        assert_eq!(
            snap_to_boundary(ideal, &boundaries, 1_000_000),
            Timestamp::from_seconds(9.8)
        );
        assert_eq!(snap_to_boundary(ideal, &boundaries, 10), ideal);
    }
}
