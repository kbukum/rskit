//! Stateful accumulation primitives with configurable flush triggers.

#![warn(missing_docs)]

/// Core accumulator implementation.
pub mod accumulator;
/// Accumulator configuration and builders.
pub mod config;
/// Multiplexed manager for keyed accumulators.
pub mod manager;
/// Value measurers used for thresholds and capacity.
pub mod measurer;
/// In-memory store implementation.
pub mod memory_store;
/// Store trait for pluggable backends.
pub mod store;
/// Flush triggers.
pub mod trigger;

pub use accumulator::Accumulator;
pub use config::AccumulatorConfig;
pub use manager::Manager;
pub use measurer::{ByteSizeMeasurer, CountMeasurer, Measurer};
pub use memory_store::MemoryStore;
pub use store::Store;
pub use trigger::{ByteSizeTrigger, SizeTrigger, TimeTrigger, Trigger};
