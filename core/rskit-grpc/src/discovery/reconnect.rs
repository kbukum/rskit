//! Background reconnect state cloned from a discovery channel.

use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{debug, warn};

use super::connector::{ChannelConnector, connect_grpc_channel};
use crate::channel::GrpcChannel;
use crate::config::GrpcClientConfig;

/// State a background task clones from a [`DiscoveryChannel`](super::DiscoveryChannel) to reconnect it:
/// the connector and base config, plus the cached target
/// and channel it rewrites when the resolved target changes.
pub(super) struct ReconnectContext {
    pub(super) service_name: String,
    pub(super) config: GrpcClientConfig,
    pub(super) connector: Arc<dyn ChannelConnector>,
    pub(super) current_target: Arc<RwLock<Option<String>>>,
    pub(super) channel: Arc<RwLock<Option<GrpcChannel>>>,
}

impl ReconnectContext {
    /// Compare `new_target` with the cached target and, if different, create a new underlying gRPC channel.
    pub(super) async fn maybe_reconnect(&self, new_target: &str) {
        let old_target = self.current_target.read().await.clone();
        if old_target.as_deref() == Some(new_target) {
            return;
        }

        debug!(
            service = %self.service_name,
            old_target = ?old_target,
            new_target = %new_target,
            "target changed, reconnecting"
        );

        let gc = match connect_grpc_channel(&self.connector, &self.config, new_target).await {
            Ok(gc) => gc,
            Err(e) => {
                warn!(
                    service = %self.service_name,
                    target = %new_target,
                    error = %e,
                    "background reconnect failed"
                );
                return;
            }
        };

        *self.current_target.write().await = Some(new_target.to_owned());
        *self.channel.write().await = Some(gc);
    }
}
