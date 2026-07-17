//! Provider traits (request-response, stream, sink, duplex) with a tower bridge.

#![warn(missing_docs)]

/// Closure-based provider adapters.
pub mod adapt;
/// Provider registry with operation and tier-based resolution.
pub mod registry;
/// [`TowerProvider`] — bridge from `tower::Service` to [`traits::RequestResponse`].
pub mod tower_bridge;
/// Core provider traits.
pub mod traits;

pub use adapt::{request_response_fn, sink_fn};
pub use registry::{Binding, Registry};
pub use tower_bridge::TowerProvider;
pub use traits::{Duplex, DuplexChannel, Provider, RequestResponse, Sink, Stream};

#[cfg(test)]
mod tests;
