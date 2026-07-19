//! Bounded async worker pool with streaming events and cooperative cancellation.

mod config;
mod queue;
mod runtime;

pub use config::{OverflowPolicy, PoolConfig, PoolStats};
pub use runtime::Pool;
