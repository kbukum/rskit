//! Service discovery with load balancing strategies.
//!
//! Resolve [`ServiceInstance`]s through the [`Discovery`] trait and
//! manage registrations via [`Registry`]. Pick an instance using one
//! of the built-in load balancers: [`RoundRobin`], [`Random`], or
//! [`LeastConnections`].

#![warn(missing_docs)]

/// Load balancing strategies.
pub mod balancer;
/// Lifecycle-managed discovery component.
#[cfg(feature = "component")]
pub mod component;
/// Service discovery configuration.
pub mod config;
/// Consul-backed service discovery.
#[cfg(feature = "consul")]
pub mod consul;
/// Provider factory registry.
pub mod factory;
/// Service instance representation.
pub mod instance;
/// In-memory discovery for testing.
pub mod memory;
/// Bootstrap-time address resolution utilities.
pub mod resolve;
/// Discovery-integrated server component.
#[cfg(feature = "server")]
pub mod server;
/// Core discovery and registry traits.
pub mod traits;

pub use balancer::{LeastConnections, LoadBalancer, Random, RoundRobin, Weighted};
#[cfg(feature = "component")]
pub use component::DiscoveryComponent;
pub use config::DiscoveryConfig;
#[cfg(feature = "consul")]
pub use consul::ConsulDiscovery;
pub use factory::{DiscoveryRegistry, ProviderFactory, ProviderPair};
pub use instance::ServiceInstance;
pub use memory::InMemoryDiscovery;
pub use resolve::resolve_addr;
#[cfg(feature = "server")]
pub use server::DiscoveryServer;
pub use traits::{Discovery, Registry, Watcher};
