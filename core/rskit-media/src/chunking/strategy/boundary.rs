//! Boundary snapping helper shared by chunk strategies.

use crate::chunking::types::ChunkBoundary;
use crate::time::Timestamp;

/// Snap a timestamp to the nearest boundary within tolerance.
pub(super) fn snap_to_boundary(
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
