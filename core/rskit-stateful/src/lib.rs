//! Typed state machines and stateful accumulation primitives.

#![warn(missing_docs)]

/// Core accumulator implementation.
pub mod accumulator;
/// Accumulator configuration and builders.
pub mod config;
/// Typed state machine primitives.
pub mod machine;
/// Multiplexed manager for keyed accumulators.
pub mod manager;
/// Value measurers used for thresholds and capacity.
pub mod measurer;
/// In-memory store implementation.
pub mod memory_store;
/// Store trait for pluggable backends.
pub mod store;
/// Shared test doubles for accumulator and manager tests.
#[cfg(test)]
mod test_support;
/// Flush triggers.
pub mod trigger;

pub use accumulator::Accumulator;
pub use config::AccumulatorConfig;
pub use machine::{AuditEntry, State, StateMachine, StatePersistence, StateSnapshot, Transition};
pub use manager::{CleanupReport, Manager};
pub use measurer::{ByteSizeMeasurer, CountMeasurer, Measurer};
pub use memory_store::MemoryStore;
pub use store::Store;
pub use trigger::{ByteSizeTrigger, SizeTrigger, TimeTrigger, Trigger};
