//! Service discovery integration for gRPC channels.

mod channel;
mod config;
mod connector;
mod reconnect;

pub use channel::DiscoveryChannel;
pub use config::DiscoveryChannelConfig;
