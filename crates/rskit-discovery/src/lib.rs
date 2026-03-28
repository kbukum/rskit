//! Service discovery with load balancing strategies.
//!
//! Register, resolve, and deregister [`ServiceInstance`]s through the
//! [`Discovery`] trait. Pick an instance using one of the built-in
//! load balancers: [`RoundRobin`], [`Random`], or [`LeastConnections`].

#![warn(missing_docs)]

/// Load balancing strategies.
pub mod balancer;
/// Service instance representation.
pub mod instance;
/// In-memory discovery for testing.
pub mod memory;
/// Core discovery trait.
pub mod traits;

pub use balancer::{LeastConnections, LoadBalancer, Random, RoundRobin};
pub use instance::ServiceInstance;
pub use memory::InMemoryDiscovery;
pub use traits::Discovery;
