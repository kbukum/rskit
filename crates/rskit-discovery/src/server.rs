//! Discovery-integrated server component.
//!
//! Wraps any server implementing the [`rskit_bootstrap::Component`] trait to automatically
//! register with service discovery on start and deregister on stop.

use std::sync::Arc;

use async_trait::async_trait;
use rskit_bootstrap::{Component, Health};
use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::{instance::ServiceInstance, traits::Registry};

/// Wraps a server component with service discovery auto-registration.
///
/// On `start()`, the inner server starts first, then the instance is registered.
/// On `stop()`, the instance is deregistered first, then the inner server stops.
pub struct DiscoveryServer<S: Component + ?Sized> {
    inner: Arc<S>,
    registry: Arc<dyn Registry>,
    instance: ServiceInstance,
    name: String,
}

impl<S: Component + ?Sized> DiscoveryServer<S> {
    /// Create a new discovery-integrated server.
    ///
    /// # Arguments
    ///
    /// * `name` - Component identifier (e.g., "discovery-grpc-server")
    /// * `inner` - The server component to wrap
    /// * `registry` - The service registry for registration/deregistration
    /// * `instance` - The service instance configuration
    pub fn new(
        name: String,
        inner: Arc<S>,
        registry: Arc<dyn Registry>,
        instance: ServiceInstance,
    ) -> Self {
        Self {
            inner,
            registry,
            instance,
            name,
        }
    }

    /// Returns a reference to the wrapped server component.
    pub fn inner(&self) -> &S {
        &self.inner
    }

    /// Returns the service instance being registered.
    pub fn instance(&self) -> &ServiceInstance {
        &self.instance
    }
}

#[async_trait]
impl<S: Component + ?Sized + 'static> Component for DiscoveryServer<S> {
    fn name(&self) -> &str {
        &self.name
    }

    async fn start(&self) -> AppResult<()> {
        // Start the inner server first
        tracing::debug!(
            component = %self.name,
            "Starting inner server component"
        );
        self.inner.start().await.map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to start inner server: {}", e),
            )
        })?;

        // Then register with discovery
        tracing::debug!(
            component = %self.name,
            service_id = %self.instance.id,
            service_name = %self.instance.name,
            address = %self.instance.address,
            port = %self.instance.port,
            "Registering with service discovery"
        );

        if let Err(err) = self.registry.register(&self.instance).await {
            // Log and attempt to stop the server if registration fails
            tracing::error!(
                component = %self.name,
                error = %err,
                "Registration failed, stopping inner server"
            );
            let inner_clone = self.inner.clone();
            tokio::spawn(async move {
                if let Err(e) = inner_clone.stop().await {
                    tracing::warn!(
                        "Failed to stop inner server after registration failure: {}",
                        e
                    );
                }
            });
            return Err(AppError::new(
                ErrorCode::Internal,
                format!("failed to register with discovery: {}", err),
            ));
        }

        tracing::debug!(
            component = %self.name,
            service_id = %self.instance.id,
            "Service registered successfully"
        );
        Ok(())
    }

    async fn stop(&self) -> AppResult<()> {
        tracing::debug!(
            component = %self.name,
            service_id = %self.instance.id,
            "Stopping discovery-server component"
        );

        // Deregister from discovery first
        if let Err(e) = self.registry.deregister(&self.instance.id).await {
            tracing::warn!(
                component = %self.name,
                service_id = %self.instance.id,
                error = %e,
                "Failed to deregister from discovery"
            );
            // Continue to stop the server even if deregistration fails
        }

        // Then stop the inner server
        self.inner.stop().await.map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to stop inner server: {}", e),
            )
        })?;

        tracing::debug!(
            component = %self.name,
            "Discovery-server component stopped"
        );
        Ok(())
    }

    fn health(&self) -> Health {
        // Combine health of discovery registration status
        // For now, delegate to inner component
        // In a real implementation, you might check if the service is still registered
        let inner_health = self.inner.health();
        if inner_health.is_healthy() {
            Health::healthy(format!("{} (registered)", self.name))
        } else {
            Health::unhealthy(
                format!("{} (inner unhealthy)", self.name),
                "inner component is unhealthy",
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use rskit_errors::AppResult;

    /// Mock server for testing
    struct MockServer {
        started: parking_lot::Mutex<bool>,
        stopped: parking_lot::Mutex<bool>,
    }

    impl MockServer {
        fn new() -> Self {
            Self {
                started: parking_lot::Mutex::new(false),
                stopped: parking_lot::Mutex::new(false),
            }
        }
    }

    #[async_trait]
    impl Component for MockServer {
        fn name(&self) -> &str {
            "mock-server"
        }

        async fn start(&self) -> AppResult<()> {
            *self.started.lock() = true;
            Ok(())
        }

        async fn stop(&self) -> AppResult<()> {
            *self.stopped.lock() = true;
            Ok(())
        }

        fn health(&self) -> Health {
            Health::healthy("mock-server")
        }
    }

    /// Mock registry for testing
    struct MockRegistry {
        registered: parking_lot::Mutex<Vec<ServiceInstance>>,
        deregistered: parking_lot::Mutex<Vec<String>>,
        register_error: parking_lot::Mutex<Option<String>>,
        deregister_error: parking_lot::Mutex<Option<String>>,
    }

    impl MockRegistry {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                registered: parking_lot::Mutex::new(Vec::new()),
                deregistered: parking_lot::Mutex::new(Vec::new()),
                register_error: parking_lot::Mutex::new(None),
                deregister_error: parking_lot::Mutex::new(None),
            })
        }

        fn set_register_error(&self, error: Option<String>) {
            *self.register_error.lock() = error;
        }

        fn set_deregister_error(&self, error: Option<String>) {
            *self.deregister_error.lock() = error;
        }

        fn registered_instances(&self) -> Vec<ServiceInstance> {
            self.registered.lock().clone()
        }

        fn deregistered_ids(&self) -> Vec<String> {
            self.deregistered.lock().clone()
        }
    }

    #[async_trait]
    impl Registry for MockRegistry {
        async fn register(&self, instance: &ServiceInstance) -> AppResult<()> {
            if let Some(err) = &*self.register_error.lock() {
                return Err(AppError::new(ErrorCode::Internal, err.clone()));
            }
            self.registered.lock().push(instance.clone());
            Ok(())
        }

        async fn deregister(&self, id: &str) -> AppResult<()> {
            if let Some(err) = &*self.deregister_error.lock() {
                return Err(AppError::new(ErrorCode::Internal, err.clone()));
            }
            self.deregistered.lock().push(id.to_string());
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_lifecycle_success() {
        let server = Arc::new(MockServer::new());
        let registry = MockRegistry::new();
        let instance = ServiceInstance {
            id: "test-1".to_string(),
            name: "test-service".to_string(),
            address: "127.0.0.1".to_string(),
            port: 8080,
            healthy: true,
            tags: vec!["test".to_string()],
            metadata: Default::default(),
        };

        let discovery_server = DiscoveryServer::new(
            "discovery-test".to_string(),
            server.clone(),
            registry.clone(),
            instance.clone(),
        );

        // Start should start inner server and register
        discovery_server.start().await.unwrap();
        assert!(*server.started.lock());
        assert_eq!(registry.registered_instances().len(), 1);
        assert_eq!(registry.registered_instances()[0].id, "test-1");

        // Stop should deregister and stop inner server
        discovery_server.stop().await.unwrap();
        assert!(*server.stopped.lock());
        assert_eq!(registry.deregistered_ids().len(), 1);
        assert_eq!(registry.deregistered_ids()[0], "test-1");
    }

    #[tokio::test]
    async fn test_registration_failure_stops_server() {
        let server = Arc::new(MockServer::new());
        let registry = MockRegistry::new();
        registry.set_register_error(Some("service unavailable".to_string()));

        let instance = ServiceInstance {
            id: "test-2".to_string(),
            name: "test-service".to_string(),
            address: "127.0.0.1".to_string(),
            port: 8081,
            healthy: true,
            tags: vec![],
            metadata: Default::default(),
        };

        let discovery_server = DiscoveryServer::new(
            "discovery-test".to_string(),
            server.clone(),
            registry.clone(),
            instance,
        );

        // Start should fail
        let result = discovery_server.start().await;
        assert!(result.is_err());

        // Server should have started (even though registration failed)
        assert!(*server.started.lock());

        // After a short delay, we should see the stop called
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        assert!(*server.stopped.lock());
    }

    #[tokio::test]
    async fn test_deregistration_failure_doesnt_prevent_stop() {
        let server = Arc::new(MockServer::new());
        let registry = MockRegistry::new();
        let instance = ServiceInstance {
            id: "test-3".to_string(),
            name: "test-service".to_string(),
            address: "127.0.0.1".to_string(),
            port: 8082,
            healthy: true,
            tags: vec![],
            metadata: Default::default(),
        };

        let discovery_server = DiscoveryServer::new(
            "discovery-test".to_string(),
            server.clone(),
            registry.clone(),
            instance,
        );

        // Start successfully
        discovery_server.start().await.unwrap();
        assert_eq!(registry.registered_instances().len(), 1);

        // Set deregistration error
        registry.set_deregister_error(Some("registry error".to_string()));

        // Stop should still succeed
        let result = discovery_server.stop().await;
        assert!(result.is_ok());

        // Server should be stopped
        assert!(*server.stopped.lock());
    }

    #[test]
    fn test_component_name_and_accessors() {
        let server = Arc::new(MockServer::new());
        let registry = MockRegistry::new();
        let instance = ServiceInstance {
            id: "test-4".to_string(),
            name: "my-service".to_string(),
            address: "192.168.1.1".to_string(),
            port: 9000,
            healthy: true,
            tags: vec!["prod".to_string()],
            metadata: Default::default(),
        };

        let discovery_server = DiscoveryServer::new(
            "my-discovery-server".to_string(),
            server,
            registry,
            instance.clone(),
        );

        assert_eq!(discovery_server.name(), "my-discovery-server");
        assert_eq!(discovery_server.instance().id, "test-4");
        assert_eq!(discovery_server.instance().name, "my-service");
        assert_eq!(discovery_server.instance().port, 9000);
    }
}
