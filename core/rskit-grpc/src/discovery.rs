#[cfg(feature = "discovery")]
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
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
    #[must_use]
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

#[async_trait]
trait ChannelConnector: Send + Sync {
    async fn connect(&self, config: GrpcClientConfig) -> AppResult<GrpcChannel>;
}

#[derive(Default)]
struct DefaultChannelConnector;

#[async_trait]
impl ChannelConnector for DefaultChannelConnector {
    async fn connect(&self, config: GrpcClientConfig) -> AppResult<GrpcChannel> {
        let channel = GrpcChannel::new(config);
        channel.connect().await?;
        Ok(channel)
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
    connector: Arc<dyn ChannelConnector>,
    /// Current connected target address.
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
    /// task. Call [`Self::start_background`] to begin automatic resolution.
    pub fn new(
        discovery: Arc<dyn Discovery>,
        service_name: impl Into<String>,
        config: GrpcClientConfig,
    ) -> Self {
        Self::build(
            discovery,
            None,
            service_name,
            DiscoveryChannelConfig::new(config),
            Arc::new(DefaultChannelConnector),
        )
    }

    /// Create a [`DiscoveryChannel`] with an optional [`Watcher`] and richer
    /// configuration.
    pub fn with_watcher(
        discovery: Arc<dyn Discovery>,
        watcher: Option<Arc<dyn Watcher>>,
        service_name: impl Into<String>,
        config: DiscoveryChannelConfig,
    ) -> Self {
        Self::build(
            discovery,
            watcher,
            service_name,
            config,
            Arc::new(DefaultChannelConnector),
        )
    }

    fn build(
        discovery: Arc<dyn Discovery>,
        watcher: Option<Arc<dyn Watcher>>,
        service_name: impl Into<String>,
        config: DiscoveryChannelConfig,
        connector: Arc<dyn ChannelConnector>,
    ) -> Self {
        let service_name = service_name.into();
        debug!(
            service = %service_name,
            "Creating DiscoveryChannel target: {}",
            config.grpc.target
        );

        Self {
            discovery,
            watcher,
            service_name,
            config,
            connector,
            current_target: Arc::new(RwLock::new(None)),
            channel: Arc::new(RwLock::new(None)),
            bg_handle: Arc::new(RwLock::new(None)),
        }
    }

    #[cfg(test)]
    fn with_connector(
        discovery: Arc<dyn Discovery>,
        watcher: Option<Arc<dyn Watcher>>,
        service_name: impl Into<String>,
        config: DiscoveryChannelConfig,
        connector: Arc<dyn ChannelConnector>,
    ) -> Self {
        Self::build(discovery, watcher, service_name, config, connector)
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
        let grpc_config = self.config.grpc.clone();
        let connector = Arc::clone(&self.connector);
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
                        &grpc_config,
                        &new_target,
                        &connector,
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
                                &grpc_config,
                                &new_target,
                                &connector,
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
        base_config: &GrpcClientConfig,
        new_target: &str,
        connector: &Arc<dyn ChannelConnector>,
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

        let gc = match connect_grpc_channel(connector, base_config, new_target).await {
            Ok(gc) => gc,
            Err(e) => {
                warn!(
                    service = %service_name,
                    target = %new_target,
                    error = %e,
                    "background reconnect failed"
                );
                return;
            }
        };

        *current_target.write().await = Some(new_target.to_owned());
        *channel.write().await = Some(gc);
    }

    /// Resolve the service to a target address using discovery.
    ///
    /// Returns the resolved address (host:port) without mutating the connected-target cache.
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

        // Pick first available instance.
        let instance = &instances[0];
        let target = instance.endpoint();

        debug!(
            service = %self.service_name,
            target = %target,
            "Resolved service to target"
        );

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
            if let Some(gc) = ch.as_ref()
                && let Ok(connected_ch) = gc.connected_channel().await
            {
                return Ok(connected_ch);
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
        let config = config_for_target(&self.config.grpc, &target);
        let gc = match self.connector.connect(config).await {
            Ok(gc) => gc,
            Err(e) => {
                // Trigger an immediate re-resolve so the *next* call can
                // benefit from an updated target.
                warn!(
                    service = %self.service_name,
                    target = %target,
                    error = %e,
                    "connection failed, triggering re-resolve"
                );
                if let Ok(new_target) = self.resolve().await
                    && new_target != target
                    && let Ok(gc2) =
                        connect_grpc_channel(&self.connector, &self.config.grpc, &new_target).await
                {
                    let connected_ch = gc2.connected_channel().await?;
                    *self.current_target.write().await = Some(new_target);
                    *self.channel.write().await = Some(gc2);
                    return Ok(connected_ch);
                }
                return Err(e);
            }
        };

        let connected_ch = gc.connected_channel().await?;

        // Cache the channel
        {
            let mut ch = self.channel.write().await;
            *ch = Some(gc);
        }
        *self.current_target.write().await = Some(target);

        Ok(connected_ch)
    }

    /// Refresh the service resolution and update the channel if target changed.
    ///
    /// Returns true if the target address changed and a new channel was created.
    /// Returns false if the target remained the same.
    pub async fn refresh(&self) -> AppResult<bool> {
        let old_target = self.current_target.read().await.clone();
        let new_target = self.resolve().await?;
        let changed = old_target.as_ref() != Some(&new_target);

        if changed {
            debug!(
                service = %self.service_name,
                old_target = ?old_target,
                new_target = %new_target,
                "Service target changed, creating new channel"
            );

            // Clear old channel and create new one
            let gc = connect_grpc_channel(&self.connector, &self.config.grpc, &new_target).await?;

            let mut ch = self.channel.write().await;
            *ch = Some(gc);
            *self.current_target.write().await = Some(new_target);
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

fn config_for_target(base_config: &GrpcClientConfig, target: &str) -> GrpcClientConfig {
    let mut config = base_config.clone();
    config.target = target.to_string();
    config
}

async fn connect_grpc_channel(
    connector: &Arc<dyn ChannelConnector>,
    base_config: &GrpcClientConfig,
    target: &str,
) -> AppResult<GrpcChannel> {
    connector
        .connect(config_for_target(base_config, target))
        .await
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;
    use async_trait::async_trait;
    use parking_lot::Mutex;
    use rskit_discovery::{Watcher, instance::ServiceInstance};
    use tokio::sync::mpsc;

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
                    weight: 1,
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
    async fn resolve_does_not_mutate_connected_target_cache() {
        let discovery = Arc::new(MockDiscovery);
        let ch = DiscoveryChannel::new(
            discovery,
            "valid-service",
            GrpcClientConfig::new("localhost:50051"),
        );

        let target = ch.resolve().await.unwrap();
        assert_eq!(target, "localhost:9090");
        assert!(ch.current_target().await.is_none());
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
    async fn test_with_watcher_constructor() {
        let discovery = Arc::new(MockDiscovery);
        let config = DiscoveryChannelConfig::new(GrpcClientConfig::new("localhost:50051"))
            .with_resolve_interval(Duration::from_secs(5));

        // No watcher provided — still works
        let ch = DiscoveryChannel::with_watcher(discovery, None, "valid-service", config);

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

    #[test]
    fn discovery_channel_config_can_be_built_from_grpc_config() {
        let cfg = DiscoveryChannelConfig::from(GrpcClientConfig::new("seed:50051"))
            .with_resolve_interval(Duration::from_millis(25));

        assert_eq!(cfg.grpc.target, "seed:50051");
        assert_eq!(cfg.resolve_interval, Duration::from_millis(25));
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
        tokio::time::pause();

        let discovery = Arc::new(StaticDiscovery::new("127.0.0.1:9090"));
        let connector = Arc::new(MockConnector::new([false, true]));
        let config = DiscoveryChannelConfig::new(GrpcClientConfig::new("localhost:50051"))
            .with_resolve_interval(Duration::from_millis(50));

        let mut ch =
            DiscoveryChannel::with_connector(discovery, None, "valid-service", config, connector);

        // Start background polling
        ch.start_background().await.unwrap();

        tokio::time::advance(Duration::from_millis(75)).await;
        tokio::task::yield_now().await;
        assert!(ch.current_target().await.is_none());

        let mut target = None;
        for _ in 0..4 {
            tokio::time::advance(Duration::from_millis(50)).await;
            tokio::task::yield_now().await;
            target = ch.current_target().await;
            if target.is_some() {
                break;
            }
        }

        // The poller retries the same discovered target until a connection succeeds.
        assert_eq!(target, Some("127.0.0.1:9090".to_string()));

        // Close should cancel the background task
        ch.close().await.unwrap();
    }

    #[tokio::test]
    async fn refresh_connects_initial_target_without_false_no_change() {
        let discovery = Arc::new(SequenceDiscovery::new(["127.0.0.1:9090"]));
        let connector = Arc::new(MockConnector::new([true]));
        let ch = DiscoveryChannel::with_connector(
            discovery,
            None,
            "valid-service",
            DiscoveryChannelConfig::new(GrpcClientConfig::new("localhost:50051")),
            connector,
        );

        let changed = ch.refresh().await.unwrap();
        assert!(changed);
        assert_eq!(
            ch.current_target().await,
            Some("127.0.0.1:9090".to_string())
        );
    }

    #[tokio::test]
    async fn refresh_reports_no_change_after_successful_connection() {
        let discovery = Arc::new(SequenceDiscovery::new(["127.0.0.1:9090", "127.0.0.1:9090"]));
        let connector = Arc::new(MockConnector::new([true]));
        let ch = DiscoveryChannel::with_connector(
            discovery,
            None,
            "valid-service",
            DiscoveryChannelConfig::new(GrpcClientConfig::new("localhost:50051")),
            connector,
        );

        assert!(ch.refresh().await.unwrap());
        assert!(!ch.refresh().await.unwrap());
    }

    #[tokio::test]
    async fn resolve_errors_when_discovery_returns_no_instances() {
        let discovery = Arc::new(EmptyDiscovery);
        let ch = DiscoveryChannel::new(
            discovery,
            "empty-service",
            GrpcClientConfig::new("localhost:50051"),
        );

        let err = ch.resolve().await.unwrap_err();

        assert_eq!(err.code(), rskit_errors::ErrorCode::ServiceUnavailable);
        assert!(err.to_string().contains("no instances"));
    }

    #[tokio::test]
    async fn refresh_propagates_connector_failure_without_caching_target() {
        let discovery = Arc::new(SequenceDiscovery::new(["127.0.0.1:9090"]));
        let connector = Arc::new(MockConnector::new([false]));
        let ch = DiscoveryChannel::with_connector(
            discovery,
            None,
            "valid-service",
            DiscoveryChannelConfig::new(GrpcClientConfig::new("localhost:50051")),
            connector,
        );

        let err = ch.refresh().await.unwrap_err();

        assert_eq!(err.code(), rskit_errors::ErrorCode::ServiceUnavailable);
        assert!(ch.current_target().await.is_none());
    }

    #[tokio::test]
    async fn watcher_background_reconnects_on_non_empty_updates_and_skips_empty_updates() {
        let discovery = Arc::new(StaticDiscovery::new("127.0.0.1:9090"));
        let (watcher, tx) = ChannelWatcher::new();
        let connector = Arc::new(MockConnector::new([true]));
        let mut ch = DiscoveryChannel::with_connector(
            discovery,
            Some(Arc::new(watcher)),
            "valid-service",
            DiscoveryChannelConfig::new(GrpcClientConfig::new("seed:50051")),
            connector,
        );

        ch.start_background().await.unwrap();
        tx.send(Vec::new()).await.expect("send empty update");
        tokio::task::yield_now().await;
        assert!(ch.current_target().await.is_none());

        tx.send(vec![service_instance("valid-service", "127.0.0.1:9443")])
            .await
            .expect("send service update");
        for _ in 0..4 {
            tokio::task::yield_now().await;
            if ch.current_target().await.is_some() {
                break;
            }
        }

        assert_eq!(
            ch.current_target().await,
            Some("127.0.0.1:9443".to_string())
        );
        assert_eq!(ch.service_name(), "valid-service");
        assert_eq!(ch.grpc_config().target, "seed:50051");
        assert_eq!(ch.config().grpc.target, "seed:50051");
        ch.start_background().await.unwrap();
        ch.close().await.unwrap();
        assert!(ch.current_target().await.is_none());
    }

    struct StaticDiscovery {
        target: String,
    }

    impl StaticDiscovery {
        fn new(target: &str) -> Self {
            Self {
                target: target.to_string(),
            }
        }
    }

    #[async_trait]
    impl Discovery for StaticDiscovery {
        async fn resolve(&self, service: &str) -> AppResult<Vec<ServiceInstance>> {
            Ok(vec![service_instance(service, &self.target)])
        }
    }

    struct EmptyDiscovery;

    #[async_trait]
    impl Discovery for EmptyDiscovery {
        async fn resolve(&self, _service: &str) -> AppResult<Vec<ServiceInstance>> {
            Ok(Vec::new())
        }
    }

    struct SequenceDiscovery {
        targets: Mutex<VecDeque<String>>,
    }

    impl SequenceDiscovery {
        fn new<const N: usize>(targets: [&str; N]) -> Self {
            Self {
                targets: Mutex::new(targets.into_iter().map(str::to_owned).collect()),
            }
        }
    }

    #[async_trait]
    impl Discovery for SequenceDiscovery {
        async fn resolve(&self, service: &str) -> AppResult<Vec<ServiceInstance>> {
            let target = {
                let mut targets = self.targets.lock();
                match targets.len() {
                    0 => "127.0.0.1:9090".to_string(),
                    1 => targets.front().cloned().unwrap(),
                    _ => targets.pop_front().unwrap(),
                }
            };
            Ok(vec![service_instance(service, &target)])
        }
    }

    struct MockConnector {
        outcomes: Mutex<VecDeque<bool>>,
    }

    impl MockConnector {
        fn new<const N: usize>(outcomes: [bool; N]) -> Self {
            Self {
                outcomes: Mutex::new(outcomes.into_iter().collect()),
            }
        }
    }

    #[async_trait]
    impl ChannelConnector for MockConnector {
        async fn connect(&self, config: GrpcClientConfig) -> AppResult<GrpcChannel> {
            let should_succeed = self.outcomes.lock().pop_front().unwrap_or(true);
            if should_succeed {
                Ok(GrpcChannel::new(config))
            } else {
                Err(rskit_errors::AppError::new(
                    rskit_errors::ErrorCode::ServiceUnavailable,
                    format!("mock connect failed for {}", config.target),
                ))
            }
        }
    }

    fn service_instance(service: &str, endpoint: &str) -> ServiceInstance {
        let (address, port) = endpoint.split_once(':').unwrap();
        ServiceInstance {
            id: format!("{service}-{endpoint}"),
            name: service.to_string(),
            address: address.to_string(),
            port: port.parse().unwrap(),
            healthy: true,
            weight: 1,
            tags: vec![],
            metadata: Default::default(),
        }
    }

    struct ChannelWatcher {
        receiver: Mutex<Option<mpsc::Receiver<Vec<ServiceInstance>>>>,
    }

    impl ChannelWatcher {
        fn new() -> (Self, mpsc::Sender<Vec<ServiceInstance>>) {
            let (tx, rx) = mpsc::channel(4);
            (
                Self {
                    receiver: Mutex::new(Some(rx)),
                },
                tx,
            )
        }
    }

    #[async_trait]
    impl Watcher for ChannelWatcher {
        async fn watch(&self, _service: &str) -> AppResult<mpsc::Receiver<Vec<ServiceInstance>>> {
            self.receiver.lock().take().ok_or_else(|| {
                rskit_errors::AppError::new(
                    rskit_errors::ErrorCode::Internal,
                    "watcher already consumed",
                )
            })
        }
    }
}
