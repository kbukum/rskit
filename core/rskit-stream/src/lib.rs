//! Foundational async stream toolkit.
//!
//! `rskit-stream` owns the opinion-free, layer-zero building blocks for the
//! "observe → fan-out → consume" graph that recurs across config reloads,
//! service discovery, cache invalidation, secret rotation, and message
//! consumers. Sources, the bounded fan-out bus, cancellable consumer tasks,
//! and the `futures::Stream` extension operators that chain them all live
//! together at the foundation where any higher layer can reuse them without
//! inverting the layer order.
//!
//! Sequential, named-step workflows (run N steps, report progress, cancel)
//! are a different concern and live in `rskit-chain`.

#![warn(missing_docs)]

/// Bounded fan-out broadcaster source (`Broadcaster<T>`).
pub mod broadcaster;
/// Extension trait adding `rskit` operators to any `Stream`.
pub mod ext;
/// Higher-level stream operators (map, filter, fan-out, windowing, etc.).
pub mod operators;
/// Terminal sink combinators (`collect`, `drain`, `for_each`).
pub mod sink;
/// Stream source constructors (`from_slice`, `from_fn`, `from_channel`).
pub mod source;
/// Cancellable owned tasks (`SpawnedTask`, `TaskGroup`).
pub mod task;

pub use broadcaster::{BroadcastStream, Broadcaster, DEFAULT_BROADCAST_BUFFER};
pub use ext::RskitStreamExt;
pub use operators::combine::{concat, merge};
pub use sink::{collect, drain, for_each};
pub use source::{from_channel, from_fn, from_slice};
pub use task::{SpawnedTask, TaskGroup};

pub use tokio_util::sync::CancellationToken;

#[cfg(test)]
mod tests;
