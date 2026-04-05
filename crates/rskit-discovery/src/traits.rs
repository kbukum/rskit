use async_trait::async_trait;

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
