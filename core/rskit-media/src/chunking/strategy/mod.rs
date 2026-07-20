//! Chunking strategy traits and built-in implementations.

mod boundary;
mod chunk_strategy;
mod fixed_duration;
mod keyframe;
mod silence;

#[cfg(test)]
mod test_support;

pub use chunk_strategy::ChunkStrategy;
pub use fixed_duration::FixedDurationStrategy;
pub use keyframe::KeyframeStrategy;
pub use silence::SilenceStrategy;
