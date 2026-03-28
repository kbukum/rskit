use async_trait::async_trait;

use rskit_errors::AppResult;

use crate::instance::ServiceInstance;

/// Service registry — resolve, register, and deregister service instances.
#[async_trait]
pub trait Discovery: Send + Sync {
    /// Return all known instances for the given service name.
    async fn resolve(&self, service: &str) -> AppResult<Vec<ServiceInstance>>;
    /// Register an instance so it can be discovered.
    async fn register(&self, instance: &ServiceInstance) -> AppResult<()>;
    /// Remove an instance by its unique id.
    async fn deregister(&self, id: &str) -> AppResult<()>;
}
