#[cfg(feature = "discovery")]
use std::sync::Arc;

use rskit_discovery::Discovery;
use rskit_errors::AppResult;
use tokio::sync::RwLock;
use tonic::transport::Channel;
use tracing::{debug, warn};

use crate::config::GrpcClientConfig;
use crate::channel::GrpcChannel;

/// Discovery-enabled gRPC channel that resolves service instances dynamically.
///
/// Maintains a gRPC channel to a service discovered via the [`Discovery`] trait.
/// If the discovered address changes, the channel is automatically refreshed on
/// next `refresh()` call.
///
/// Mirrors `DiscoveryChannel` from pykit-grpc.
pub struct DiscoveryChannel {
    discovery: Arc<dyn Discovery>,
    service_name: String,
    config: GrpcClientConfig,
    /// Current cached target address
    current_target: Arc<RwLock<Option<String>>>,
    /// Current channel to the resolved target
    channel: Arc<RwLock<Option<GrpcChannel>>>,
}

impl DiscoveryChannel {
    /// Create a new [`DiscoveryChannel`] from a Discovery provider and service name.
    pub fn new(
        discovery: Arc<dyn Discovery>,
        service_name: impl Into<String>,
        config: GrpcClientConfig,
    ) -> Self {
        let service_name = service_name.into();
        debug!(
            service = %service_name,
            "Creating DiscoveryChannel with config target: {}",
            config.target
        );

        Self {
            discovery,
            service_name,
            config,
            current_target: Arc::new(RwLock::new(None)),
            channel: Arc::new(RwLock::new(None)),
        }
    }

    /// Resolve the service to a target address using discovery.
    ///
    /// Returns the resolved address (host:port). Updates internal cache if successful.
    pub async fn resolve(&self) -> AppResult<String> {
        debug!(service = %self.service_name, "Resolving service");

        let instances = self
            .discovery
            .resolve(&self.service_name)
            .await
            .map_err(|e| {
                warn!(
                    service = %self.service_name,
                    error = %e,
                    "Service discovery failed"
                );
                e
            })?;

        if instances.is_empty() {
            return Err(rskit_errors::AppError::new(
                rskit_errors::ErrorCode::ServiceUnavailable,
                format!("no instances found for service: {}", self.service_name),
            ));
        }

        // Pick first available instance (in production, could implement load balancing)
        let instance = &instances[0];
        let target = instance.endpoint();

        debug!(
            service = %self.service_name,
            target = %target,
            "Resolved service to target"
        );

        // Update cache
        *self.current_target.write().await = Some(target.clone());

        Ok(target)
    }

    /// Get a connected channel to the resolved service.
    ///
    /// Performs discovery and connection if needed. Returns an error if
    /// discovery or connection fails.
    pub async fn channel(&self) -> AppResult<Channel> {
        // Check if we have a cached channel
        {
            let ch = self.channel.read().await;
            if let Some(gc) = ch.as_ref() {
                if let Ok(connected_ch) = gc.connected_channel().await {
                    return Ok(connected_ch);
                }
            }
        }

        // Need to resolve or reconnect
        let target = {
            let current = self.current_target.read().await;
            current.clone()
        };

        let target = match target {
            Some(t) => t,
            None => {
                // First-time resolution
                self.resolve().await?
            }
        };

        // Create or update channel
        let config = GrpcClientConfig::new(&target);
        let gc = GrpcChannel::new(config);
        gc.connect().await?;

        let connected_ch = gc.connected_channel().await?;

        // Cache the channel
        {
            let mut ch = self.channel.write().await;
            *ch = Some(gc);
        }

        Ok(connected_ch)
    }

    /// Refresh the service resolution and update the channel if target changed.
    ///
    /// Returns true if the target address changed and a new channel was created.
    /// Returns false if the target remained the same.
    pub async fn refresh(&self) -> AppResult<bool> {
        let new_target = self.resolve().await?;

        let old_target = self.current_target.read().await;
        let changed = old_target.as_ref() != Some(&new_target);

        if changed {
            debug!(
                service = %self.service_name,
                old_target = ?*old_target,
                new_target = %new_target,
                "Service target changed, creating new channel"
            );

            // Clear old channel and create new one
            let config = GrpcClientConfig::new(&new_target);
            let gc = GrpcChannel::new(config);
            gc.connect().await?;

            let mut ch = self.channel.write().await;
            *ch = Some(gc);
        }

        Ok(changed)
    }

    /// Close the channel and clear cached state.
    pub async fn close(&mut self) -> AppResult<()> {
        debug!(service = %self.service_name, "Closing DiscoveryChannel");

        let mut ch = self.channel.write().await;
        if let Some(mut gc) = ch.take() {
            gc.close().await?;
        }

        let mut target = self.current_target.write().await;
        *target = None;

        Ok(())
    }

    /// Get the service name.
    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    /// Get the currently cached target (if any).
    pub async fn current_target(&self) -> Option<String> {
        self.current_target.read().await.clone()
    }

    /// Get the configuration.
    pub fn config(&self) -> &GrpcClientConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use rskit_discovery::instance::ServiceInstance;

    struct MockDiscovery;

    #[async_trait]
    impl Discovery for MockDiscovery {
        async fn resolve(&self, service: &str) -> AppResult<Vec<ServiceInstance>> {
            if service == "valid-service" {
                Ok(vec![ServiceInstance {
                    id: "test-instance".to_string(),
                    name: service.to_string(),
                    address: "localhost".to_string(),
                    port: 9090,
                    healthy: true,
                    tags: vec![],
                    metadata: Default::default(),
                }])
            } else {
                Err(rskit_errors::AppError::new(
                    rskit_errors::ErrorCode::ServiceUnavailable,
                    format!("service not found: {}", service),
                ))
            }
        }
    }

    #[tokio::test]
    async fn test_resolve_success() {
        let discovery = Arc::new(MockDiscovery);
        let ch = DiscoveryChannel::new(
            discovery,
            "valid-service",
            GrpcClientConfig::new("localhost:50051"),
        );

        let target = ch.resolve().await;
        assert!(target.is_ok());
        assert_eq!(target.unwrap(), "localhost:9090");
    }

    #[tokio::test]
    async fn test_resolve_failure() {
        let discovery = Arc::new(MockDiscovery);
        let ch = DiscoveryChannel::new(
            discovery,
            "unknown-service",
            GrpcClientConfig::new("localhost:50051"),
        );

        let target = ch.resolve().await;
        assert!(target.is_err());
    }

    #[tokio::test]
    async fn test_refresh_no_change() {
        let discovery = Arc::new(MockDiscovery);
        let ch = DiscoveryChannel::new(
            discovery,
            "valid-service",
            GrpcClientConfig::new("localhost:50051"),
        );

        // First resolve to get the initial target
        let initial = ch.resolve().await.unwrap();
        assert_eq!(initial, "localhost:9090");

        // Calling refresh again should resolve again and find no change
        let changed = ch.refresh().await;
        assert!(changed.is_ok());
        // The target is the same ("localhost:9090"), so changed should be false
        assert!(!changed.unwrap());
    }
}
