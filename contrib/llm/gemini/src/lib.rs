#![warn(missing_docs)]

//! Google Gemini provider (Generative Language API).

mod adapter;
mod config;
pub(crate) mod dialect;

pub(crate) const PROVIDER_ID: &str = "gemini";

#[cfg(test)]
mod fixture_tests;

pub use adapter::register;
pub use config::Config;
