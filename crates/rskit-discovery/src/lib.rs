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
pub mod server;
/// Core discovery and registry traits.
pub mod traits;

pub use balancer::{LeastConnections, LoadBalancer, Random, RoundRobin};
pub use component::DiscoveryComponent;
pub use config::DiscoveryConfig;
#[cfg(feature = "consul")]
pub use consul::ConsulDiscovery;
pub use factory::{ProviderFactory, ProviderPair, create_provider, register_provider};
pub use instance::ServiceInstance;
pub use memory::InMemoryDiscovery;
pub use resolve::resolve_addr;
pub use server::DiscoveryServer;
pub use traits::{Discovery, Registry, Watcher};
