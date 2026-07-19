//! Configuration for a discovery-enabled gRPC channel.

use std::time::Duration;

use crate::config::GrpcClientConfig;

/// Configuration for a [`DiscoveryChannel`](super::DiscoveryChannel).
#[derive(Clone, Debug)]
pub struct DiscoveryChannelConfig {
    /// Base gRPC client configuration.
    pub grpc: GrpcClientConfig,
    /// How often to poll for changes when no watcher is available. Defaults to 10 seconds.
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
