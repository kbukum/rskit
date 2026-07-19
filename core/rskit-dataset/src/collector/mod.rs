//! Collector — orchestrates source → transform → target pipelines.
//!
//! Uses an event-driven worker pool for parallel source fetching.
//! The main loop owns all mutable state (result, manifest, progress) — workers communicate via channels
//! and never touch shared mutable state.

mod config;
mod engine;
mod event;
mod progress;
mod result;
#[cfg(test)]
mod tests;
mod worker;

pub use config::CollectorConfig;
pub use engine::Collector;
pub use progress::{NullProgress, ProgressCallback};
pub use result::CollectorResult;
