#![warn(missing_docs)]

//! Anthropic Claude provider (chat messages API).

mod adapter;
mod config;
pub(crate) mod dialect;

pub(crate) const PROVIDER_ID: &str = "anthropic";

#[cfg(test)]
mod fixture_tests;

pub use adapter::register;
pub use config::Config;
