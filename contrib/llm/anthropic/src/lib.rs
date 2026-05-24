#![warn(missing_docs)]

//! Anthropic Claude provider (chat messages API).

mod adapter;
mod config;
pub(crate) mod dialect;

#[cfg(test)]
mod fixture_tests;

pub use adapter::register;
pub use config::Config;
