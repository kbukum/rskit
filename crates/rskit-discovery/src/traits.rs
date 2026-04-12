use async_trait::async_trait;
use tokio::sync::mpsc;

use rskit_errors::AppResult;

use crate::instance::ServiceInstance;

/// Service discovery — resolve service instances by name.
#[async_trait]
pub trait Discovery: Send + Sync {
    /// Return all known instances for the given service name.
    async fn resolve(&self, service: &str) -> AppResult<Vec<ServiceInstance>>;
}

/// Service registry — register and deregister service instances.
#[async_trait]
pub trait Registry: Send + Sync {
    /// Register an instance so it can be discovered.
    async fn register(&self, instance: &ServiceInstance) -> AppResult<()>;
    /// Remove an instance by its unique id.
    async fn deregister(&self, id: &str) -> AppResult<()>;
}

/// Optional extension — continuous service-set monitoring.
///
/// Implementors emit an updated instance list whenever it changes, using
/// mechanisms like Consul blocking queries, etcd watch, or polling.
#[async_trait]
pub trait Watcher: Send + Sync {
    /// Returns a channel that fires whenever the instance list changes.
    /// Implementors should use Consul blocking queries, etcd watch, etc.
    async fn watch(
        &self,
        service: &str,
    ) -> AppResult<mpsc::Receiver<Vec<ServiceInstance>>>;
}
