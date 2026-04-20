//! Core types for chunked media processing.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::time::{TimeRange, Timestamp};

/// Unique identifier for a chunk within a chunked operation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChunkId(pub String);

impl ChunkId {
    /// Create a chunk ID from an index.
    pub fn from_index(index: usize) -> Self {
        Self(format!("chunk-{index:04}"))
    }
}

impl std::fmt::Display for ChunkId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A boundary point for splitting media.
///
/// Represents a precise point in the media where a split can occur.
/// Keyframe boundaries are preferred for lossless splits.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ChunkBoundary {
    /// The timestamp of the boundary.
    pub timestamp: Timestamp,
    /// Whether this boundary falls on a keyframe.
    pub is_keyframe: bool,
    /// Confidence that this is a good split point (0.0–1.0).
    /// Higher values indicate cleaner boundaries (e.g., silence, scene change).
    pub quality: f32,
}

/// A planned chunk — describes a segment to process independently.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkPlan {
    /// Unique identifier for this chunk.
    pub id: ChunkId,
    /// Zero-based index in the chunk sequence.
    pub index: usize,
    /// Time range of this chunk within the source.
    pub range: TimeRange,
    /// Whether the start boundary is on a keyframe.
    pub start_is_keyframe: bool,
    /// Suggested timeout for this chunk.
    pub suggested_timeout: Duration,
}

impl ChunkPlan {
    /// Duration of this chunk.
    pub fn duration(&self) -> Duration {
        self.range.duration()
    }
}

/// Result of processing a single chunk.
#[derive(Debug, Clone)]
pub struct ChunkResult {
    /// Which chunk this result belongs to.
    pub id: ChunkId,
    /// Index in the sequence.
    pub index: usize,
    /// The original time range of this chunk.
    pub range: TimeRange,
    /// Status of the chunk.
    pub status: ChunkStatus,
    /// Output path for the processed chunk (if successful).
    pub output_path: Option<PathBuf>,
    /// Processing duration (wall clock).
    pub processing_duration: Duration,
    /// Arbitrary metadata produced during processing.
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Status of a chunk after processing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChunkStatus {
    /// Chunk processed successfully.
    Completed,
    /// Chunk processing failed.
    Failed {
        /// Error message.
        message: String,
        /// Whether this failure is retryable.
        retryable: bool,
    },
    /// Chunk was cancelled.
    Cancelled,
}

impl ChunkStatus {
    /// Whether the chunk completed successfully.
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Completed)
    }
}

/// Progress report for a single chunk.
#[derive(Debug, Clone)]
pub struct ChunkProgress {
    /// Which chunk this progress belongs to.
    pub chunk_id: ChunkId,
    /// Index in the sequence.
    pub chunk_index: usize,
    /// Total number of chunks.
    pub total_chunks: usize,
    /// Progress within this chunk (0.0 – 1.0).
    pub chunk_percent: f32,
    /// Overall progress across all chunks (0.0 – 1.0).
    pub overall_percent: f32,
    /// Estimated time remaining for the entire operation.
    pub eta: Option<Duration>,
}

/// A complete chunked operation plan — all chunks and reassembly info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkedOperation {
    /// Ordered list of chunk plans.
    pub chunks: Vec<ChunkPlan>,
    /// How to reassemble the chunks after processing.
    pub reassembly: ReassemblyPlan,
    /// Total duration of the source media.
    pub total_duration: Duration,
    /// The strategy that was used to create this plan.
    pub strategy_name: String,
}

impl ChunkedOperation {
    /// Number of chunks in this operation.
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// Whether this is a trivial single-chunk operation (no actual splitting needed).
    pub fn is_single_chunk(&self) -> bool {
        self.chunks.len() <= 1
    }
}

/// Describes how to reassemble processed chunks into the final output.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ReassemblyPlan {
    /// Concatenate chunks in order using FFmpeg concat demuxer.
    /// This is the standard reassembly for video/audio segments.
    Concat,
    /// Merge text-based results (e.g., transcript segments).
    /// Each chunk produces text output with time offsets that need adjustment.
    MergeText {
        /// Separator between merged chunks.
        separator: String,
    },
    /// Collect results without merging (e.g., thumbnail extraction per chunk).
    /// Each chunk produces independent output files.
    Collect,
    /// No reassembly needed — each chunk result is independent.
    None,
}
