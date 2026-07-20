//! The chunk strategy trait.

use std::time::Duration;

use rskit_errors::AppResult;

use crate::chunking::types::{ChunkBoundary, ChunkedOperation};
use crate::probe::MediaMetadata;

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
