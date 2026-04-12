//! Service discovery with load balancing strategies.
//!
//! Resolve [`ServiceInstance`]s through the [`Discovery`] trait and
//! manage registrations via [`Registry`]. Pick an instance using one
//! of the built-in load balancers: [`RoundRobin`], [`Random`], or
//! [`LeastConnections`].

#![warn(missing_docs)]

/// Service discovery configuration.
pub mod config;
/// Load balancing strategies.
pub mod balancer;
/// Consul-backed service discovery.
#[cfg(feature = "consul")]
pub mod consul;
/// Service instance representation.
pub mod instance;
/// In-memory discovery for testing.
pub mod memory;
/// Discovery-integrated server component.
pub mod server;
/// Core discovery and registry traits.
pub mod traits;

pub use balancer::{LeastConnections, LoadBalancer, Random, RoundRobin};
pub use config::DiscoveryConfig;
#[cfg(feature = "consul")]
pub use consul::ConsulDiscovery;
pub use instance::ServiceInstance;
pub use memory::InMemoryDiscovery;
pub use server::DiscoveryServer;
pub use traits::{Discovery, Registry, Watcher};
