//! Anthropic Claude provider (chat messages API).

mod adapter;
mod config;
mod dialect;

pub use adapter::new_adapter;
pub use config::Config;
pub use dialect::AnthropicDialect;
