//! Channel connector abstraction and connection helpers for discovery.

use std::sync::Arc;

use async_trait::async_trait;
use rskit_errors::AppResult;

use crate::channel::GrpcChannel;
use crate::config::GrpcClientConfig;

#[async_trait]
pub(super) trait ChannelConnector: Send + Sync {
    async fn connect(&self, config: GrpcClientConfig) -> AppResult<GrpcChannel>;
}

#[derive(Default)]
pub(super) struct DefaultChannelConnector;

#[async_trait]
impl ChannelConnector for DefaultChannelConnector {
    async fn connect(&self, config: GrpcClientConfig) -> AppResult<GrpcChannel> {
        let channel = GrpcChannel::new(config);
        channel.connect().await?;
        Ok(channel)
    }
}

pub(super) async fn connect_grpc_channel(
    connector: &Arc<dyn ChannelConnector>,
    base_config: &GrpcClientConfig,
    target: &str,
) -> AppResult<GrpcChannel> {
    connector
        .connect(config_for_target(base_config, target))
        .await
}

pub(super) fn config_for_target(base_config: &GrpcClientConfig, target: &str) -> GrpcClientConfig {
    let mut config = base_config.clone();
    config.target = target.to_string();
    config
}
