#![warn(missing_docs)]

//! Anthropic Claude provider (chat messages API).

mod adapter;
mod config;
mod dialect;

#[cfg(test)]
mod fixture_tests;

pub use adapter::{AnthropicAdapter, new_adapter};
pub use config::Config;
pub use dialect::AnthropicDialect;
