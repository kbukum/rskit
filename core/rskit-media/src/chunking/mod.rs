//! Chunked media processing — split, process in parallel, reassemble.
//!
//! This module provides the vocabulary types and strategy traits for chunked media processing.
//! The actual splitting/reassembly is performed by backends (e.g., `rskit-media-ffmpeg`).
//!
//! # Overview
//!
//! Long media files benefit from chunked processing:
//! - **Parallel processing**: Split into N chunks, process concurrently.
//! - **Per-chunk timeout**: A single chunk failure doesn't lose all progress.
//! - **Per-chunk retry**: Retry only the failed chunk, not the whole file.
//! - **Progress reporting**: Report progress per chunk and overall.
//!
//! # Architecture
//!
//! ```text
//! Source ──► ChunkStrategy.plan() ──► Vec<ChunkPlan>
//!                                         │
//!                               ┌─────────┼─────────┐
//!                               ▼         ▼         ▼
//!                          Chunk 0    Chunk 1    Chunk 2
//!                          (process)  (process)  (process)
//!                               │         │         │
//!                               └─────────┼─────────┘
//!                                         ▼
//!                                    Reassemble
//! ```

mod strategy;
mod types;

pub use strategy::{ChunkStrategy, FixedDurationStrategy, KeyframeStrategy, SilenceStrategy};
pub use types::{
    ChunkBoundary, ChunkId, ChunkPlan, ChunkProgress, ChunkResult, ChunkStatus, ChunkedOperation,
    ReassemblyPlan,
};
