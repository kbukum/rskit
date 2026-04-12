#[cfg(feature = "discovery")]
use std::sync::Arc;
use std::time::Duration;

use rskit_discovery::Discovery;
#[cfg(feature = "discovery")]
use rskit_discovery::Watcher;
use rskit_errors::AppResult;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tonic::transport::Channel;
use tracing::{debug, warn};

use crate::channel::GrpcChannel;
use crate::config::GrpcClientConfig;

/// Configuration for a [`DiscoveryChannel`].
#[derive(Clone, Debug)]
pub struct DiscoveryChannelConfig {
    /// Base gRPC client configuration.
    pub grpc: GrpcClientConfig,
    /// How often to poll for changes when no watcher is available.
    /// Defaults to 10 seconds.
    pub resolve_interval: Duration,
}

impl DiscoveryChannelConfig {
    /// Create a config with defaults and the given gRPC settings.
    pub fn new(grpc: GrpcClientConfig) -> Self {
        Self {
            grpc,
            resolve_interval: Duration::from_secs(10),
        }
    }

    /// Set the resolve polling interval.
    pub fn with_resolve_interval(mut self, interval: Duration) -> Self {
        self.resolve_interval = interval;
        self
    }
}

impl From<GrpcClientConfig> for DiscoveryChannelConfig {
    fn from(grpc: GrpcClientConfig) -> Self {
        Self::new(grpc)
    }
}

/// Discovery-enabled gRPC channel that resolves service instances dynamically.
///
/// Maintains a gRPC channel to a service discovered via the [`Discovery`] trait.
/// When an optional [`Watcher`] is provided the channel reacts to instance-set
/// changes in real-time; otherwise it falls back to periodic polling.
///
/// Mirrors `DiscoveryChannel` from pykit-grpc.
pub struct DiscoveryChannel {
    discovery: Arc<dyn Discovery>,
    watcher: Option<Arc<dyn Watcher>>,
    service_name: String,
    config: DiscoveryChannelConfig,
    /// Current cached target address
    current_target: Arc<RwLock<Option<String>>>,
    /// Current channel to the resolved target
    channel: Arc<RwLock<Option<GrpcChannel>>>,
    /// Handle to the background auto-reconnect task (if started).
    bg_handle: Arc<RwLock<Option<JoinHandle<()>>>>,
}

impl DiscoveryChannel {
    /// Create a new [`DiscoveryChannel`] from a Discovery provider and service name.
    ///
    /// This constructor preserves the original API: no watcher, no background
    /// task.  Call [`start_background`] to begin automatic resolution.
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
            watcher: None,
            service_name,
            config: DiscoveryChannelConfig::new(config),
            current_target: Arc::new(RwLock::new(None)),
            channel: Arc::new(RwLock::new(None)),
            bg_handle: Arc::new(RwLock::new(None)),
        }
    }

    /// Create a [`DiscoveryChannel`] with an optional [`Watcher`] and richer
    /// configuration.
    pub fn with_watcher(
        discovery: Arc<dyn Discovery>,
        watcher: Option<Arc<dyn Watcher>>,
        service_name: impl Into<String>,
        config: DiscoveryChannelConfig,
    ) -> Self {
        let service_name = service_name.into();
        debug!(
            service = %service_name,
            "Creating DiscoveryChannel (with_watcher) target: {}",
            config.grpc.target
        );

        Self {
            discovery,
            watcher,
            service_name,
            config,
            current_target: Arc::new(RwLock::new(None)),
            channel: Arc::new(RwLock::new(None)),
            bg_handle: Arc::new(RwLock::new(None)),
        }
    }

    /// Spawn the background auto-reconnect task.
    ///
    /// - If a [`Watcher`] was provided, listens on the watch channel.
    /// - Otherwise, periodically calls `resolve()` and reconnects on change.
    ///
    /// This is intentionally *not* called from `new()` to keep the constructor
    /// synchronous and to let callers decide when the task starts.
    pub async fn start_background(&self) -> AppResult<()> {
        // Don't double-start
        {
            let guard = self.bg_handle.read().await;
            if guard.is_some() {
                return Ok(());
            }
        }

        let discovery = Arc::clone(&self.discovery);
        let service_name = self.service_name.clone();
        let current_target = Arc::clone(&self.current_target);
        let channel = Arc::clone(&self.channel);
        let resolve_interval = self.config.resolve_interval;

        let handle = if let Some(watcher) = &self.watcher {
            // ── Watcher path ────────────────────────────────────────────
            let mut rx = watcher.watch(&self.service_name).await?;

            tokio::spawn(async move {
                while let Some(instances) = rx.recv().await {
                    if instances.is_empty() {
                        debug!(
                            service = %service_name,
                            "watcher: received empty instance list, skipping"
                        );
                        continue;
                    }
                    let new_target = instances[0].endpoint();
                    Self::maybe_reconnect(
                        &service_name,
                        &new_target,
                        &current_target,
                        &channel,
                    )
                    .await;
                }
                debug!(
                    service = %service_name,
                    "watcher channel closed, background task exiting"
                );
            })
        } else {
            // ── Polling path ────────────────────────────────────────────
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(resolve_interval).await;

                    match discovery.resolve(&service_name).await {
                        Ok(instances) if !instances.is_empty() => {
                            let new_target = instances[0].endpoint();
                            Self::maybe_reconnect(
                                &service_name,
                                &new_target,
                                &current_target,
                                &channel,
                            )
                            .await;
                        }
                        Ok(_) => {
                            debug!(
                                service = %service_name,
                                "poll: no instances found"
                            );
                        }
                        Err(e) => {
                            warn!(
                                service = %service_name,
                                error = %e,
                                "poll: resolve failed"
                            );
                        }
                    }
                }
            })
        };

        *self.bg_handle.write().await = Some(handle);
        Ok(())
    }

    /// Compare `new_target` with the cached target and, if different, create a
    /// new underlying gRPC channel.
    async fn maybe_reconnect(
        service_name: &str,
        new_target: &str,
        current_target: &Arc<RwLock<Option<String>>>,
        channel: &Arc<RwLock<Option<GrpcChannel>>>,
    ) {
        let old_target = current_target.read().await.clone();
        if old_target.as_deref() == Some(new_target) {
            return;
        }

        debug!(
            service = %service_name,
            old_target = ?old_target,
            new_target = %new_target,
            "target changed, reconnecting"
        );

        let config = GrpcClientConfig::new(new_target);
        let gc = GrpcChannel::new(config);
        if let Err(e) = gc.connect().await {
            warn!(
                service = %service_name,
                target = %new_target,
                error = %e,
                "background reconnect failed"
            );
            return;
        }

        *current_target.write().await = Some(new_target.to_owned());
        *channel.write().await = Some(gc);
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
    /// Performs discovery and connection if needed.  On connection failure the
    /// method triggers an immediate re-resolve before returning the error so
    /// that the next call can benefit from an updated target.
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

        match gc.connect().await {
            Ok(()) => {}
            Err(e) => {
                // Trigger an immediate re-resolve so the *next* call can
                // benefit from an updated target.
                warn!(
                    service = %self.service_name,
                    target = %target,
                    error = %e,
                    "connection failed, triggering re-resolve"
                );
                if let Ok(new_target) = self.resolve().await {
                    if new_target != target {
                        let gc2 = GrpcChannel::new(GrpcClientConfig::new(&new_target));
                        if gc2.connect().await.is_ok() {
                            let connected_ch = gc2.connected_channel().await?;
                            *self.channel.write().await = Some(gc2);
                            return Ok(connected_ch);
                        }
                    }
                }
                return Err(e);
            }
        }

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

    /// Close the channel, cancel the background task, and clear cached state.
    pub async fn close(&mut self) -> AppResult<()> {
        debug!(service = %self.service_name, "Closing DiscoveryChannel");

        // Cancel background task first
        {
            let mut bg = self.bg_handle.write().await;
            if let Some(handle) = bg.take() {
                handle.abort();
                // Best-effort wait; ignore JoinError from abort.
                let _ = handle.await;
            }
        }

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

    /// Get the gRPC configuration.
    pub fn grpc_config(&self) -> &GrpcClientConfig {
        &self.config.grpc
    }

    /// Get the full discovery-channel configuration.
    pub fn config(&self) -> &DiscoveryChannelConfig {
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

    #[tokio::test]
    async fn test_with_watcher_constructor() {
        let discovery = Arc::new(MockDiscovery);
        let config =
            DiscoveryChannelConfig::new(GrpcClientConfig::new("localhost:50051"))
                .with_resolve_interval(Duration::from_secs(5));

        // No watcher provided — still works
        let ch = DiscoveryChannel::with_watcher(
            discovery,
            None,
            "valid-service",
            config,
        );

        let target = ch.resolve().await;
        assert!(target.is_ok());
        assert_eq!(target.unwrap(), "localhost:9090");
    }

    #[tokio::test]
    async fn test_discovery_channel_config_defaults() {
        let cfg = DiscoveryChannelConfig::new(GrpcClientConfig::new("localhost:50051"));
        assert_eq!(cfg.resolve_interval, Duration::from_secs(10));
        assert_eq!(cfg.grpc.target, "localhost:50051");
    }

    #[tokio::test]
    async fn test_close_without_background() {
        let discovery = Arc::new(MockDiscovery);
        let mut ch = DiscoveryChannel::new(
            discovery,
            "valid-service",
            GrpcClientConfig::new("localhost:50051"),
        );

        // Resolve, then close — should not panic
        let _ = ch.resolve().await;
        let result = ch.close().await;
        assert!(result.is_ok());
        assert!(ch.current_target().await.is_none());
    }

    #[tokio::test]
    async fn test_start_background_polling() {
        let discovery = Arc::new(MockDiscovery);
        let config =
            DiscoveryChannelConfig::new(GrpcClientConfig::new("localhost:50051"))
                .with_resolve_interval(Duration::from_millis(50));

        let mut ch = DiscoveryChannel::with_watcher(
            discovery,
            None,
            "valid-service",
            config,
        );

        // Start background polling
        ch.start_background().await.unwrap();

        // Give the poller time to run at least once
        tokio::time::sleep(Duration::from_millis(200)).await;

        // The poller should have resolved the target
        let target = ch.current_target().await;
        assert_eq!(target, Some("localhost:9090".to_string()));

        // Close should cancel the background task
        ch.close().await.unwrap();
    }
}
