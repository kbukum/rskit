//! Shared test fixtures for chunk strategy tests.

use std::time::Duration;

use crate::chunking::types::ChunkBoundary;
use crate::format::Format;
use crate::probe::MediaMetadata;
use crate::time::Timestamp;
use crate::types::MediaType;

pub(super) fn make_metadata(duration_secs: f64) -> MediaMetadata {
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

pub(super) fn make_keyframe_boundaries(
    duration_secs: f64,
    interval_secs: f64,
) -> Vec<ChunkBoundary> {
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
