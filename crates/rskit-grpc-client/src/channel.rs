use std::sync::Arc;

use rskit_errors::AppResult;
use tokio::sync::RwLock;
use tonic::transport::{Channel, Endpoint};
use tracing::{debug, warn};

use crate::config::GrpcClientConfig;

/// Lazy-connecting gRPC channel wrapper.
///
/// Manages a [`tonic::transport::Channel`] with configuration, lifecycle,
/// and connectivity checks. Connection is established lazily on first use.
///
/// Mirrors the `GrpcChannel` from pykit-grpc and gokit/grpc/client patterns.
pub struct GrpcChannel {
    config: GrpcClientConfig,
    channel: Arc<RwLock<Option<Channel>>>,
}

impl GrpcChannel {
    /// Create a new [`GrpcChannel`] with the given configuration.
    pub fn new(config: GrpcClientConfig) -> Self {
        debug!("Creating GrpcChannel to {}", config.target);
        Self {
            config,
            channel: Arc::new(RwLock::new(None)),
        }
    }

    /// Establish a connection to the gRPC server.
    ///
    /// If a connection already exists, this is a no-op.
    /// Otherwise, creates a new connection using the configured endpoint.
    pub async fn connect(&self) -> AppResult<()> {
        // Check if already connected
        {
            let ch = self.channel.read().await;
            if ch.is_some() {
                debug!("GrpcChannel already connected to {}", self.config.target);
                return Ok(());
            }
        }

        // Create new connection
        debug!(
            target = %self.config.target,
            timeout = ?self.config.connect_timeout,
            "Connecting gRPC channel"
        );

        let mut endpoint = Endpoint::from_shared(format!("http://{}", self.config.target))
            .map_err(|e| {
                rskit_errors::AppError::new(
                    rskit_errors::ErrorCode::InvalidInput,
                    format!("invalid gRPC endpoint: {}", e),
                )
                .with_cause(e)
            })?;

        // Set timeouts
        endpoint = endpoint.timeout(self.config.timeout);
        endpoint = endpoint.connect_timeout(self.config.connect_timeout);

        // Configure keepalive if specified
        if let Some(interval) = self.config.keepalive_interval {
            if let Some(timeout) = self.config.keepalive_timeout {
                endpoint = endpoint.keep_alive_while_idle(true);
                endpoint = endpoint.http2_keep_alive_interval(interval);
                endpoint = endpoint.keep_alive_timeout(timeout);
            }
        }

        // Connect
        let ch = endpoint.connect().await.map_err(|e| {
            warn!(
                target = %self.config.target,
                error = %e,
                "Failed to connect gRPC channel"
            );
            rskit_errors::AppError::service_unavailable(&self.config.target).with_cause(e)
        })?;

        // Store the channel
        {
            let mut channel_guard = self.channel.write().await;
            *channel_guard = Some(ch);
        }

        debug!("GrpcChannel connected to {}", self.config.target);
        Ok(())
    }

    /// Get a reference to the underlying tonic Channel.
    ///
    /// Panics if called before [`connect`](Self::connect) succeeds.
    /// Use [`connected_channel`](Self::connected_channel) for a Result-based variant.
    pub async fn channel(&self) -> AppResult<Channel> {
        self.connected_channel().await
    }

    /// Get a reference to the underlying tonic Channel, establishing connection if needed.
    ///
    /// This is the primary method for obtaining a channel for RPC calls.
    pub async fn connected_channel(&self) -> AppResult<Channel> {
        // Check if already connected
        {
            let ch = self.channel.read().await;
            if let Some(channel) = ch.as_ref() {
                return Ok(channel.clone());
            }
        }

        // Connect if not yet connected
        self.connect().await?;

        let ch = self.channel.read().await;
        ch.clone().ok_or_else(|| {
            rskit_errors::AppError::new(
                rskit_errors::ErrorCode::Internal,
                "channel disappeared after connect",
            )
        })
    }

    /// Check if the channel is ready for RPCs.
    ///
    /// Returns true if connected, false otherwise.
    pub async fn is_ready(&self) -> bool {
        self.connected_channel().await.is_ok()
    }

    /// Close the channel gracefully.
    pub async fn close(&mut self) -> AppResult<()> {
        debug!("Closing GrpcChannel to {}", self.config.target);
        let mut ch = self.channel.write().await;
        if ch.is_some() {
            *ch = None;
        }
        Ok(())
    }

    /// Get the target address.
    pub fn target(&self) -> &str {
        &self.config.target
    }

    /// Get the configuration.
    pub fn config(&self) -> &GrpcClientConfig {
        &self.config
    }
}

impl Clone for GrpcChannel {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            channel: Arc::clone(&self.channel),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_channel() {
        let config = GrpcClientConfig::new("localhost:50051");
        let channel = GrpcChannel::new(config);
        assert_eq!(channel.target(), "localhost:50051");
    }

    #[test]
    fn test_clone_channel() {
        let config = GrpcClientConfig::new("localhost:50051");
        let channel1 = GrpcChannel::new(config);
        let channel2 = channel1.clone();
        assert_eq!(channel2.target(), "localhost:50051");
    }

    #[tokio::test]
    async fn test_is_ready_disconnected() {
        let config = GrpcClientConfig::new("localhost:99999");
        let channel = GrpcChannel::new(config);
        // Should be false since we can't connect to non-existent server
        assert!(!channel.is_ready().await);
    }
}
