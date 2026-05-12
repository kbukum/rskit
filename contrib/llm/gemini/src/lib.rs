#![warn(missing_docs)]

//! Google Gemini provider (Generative Language API).

mod adapter;
mod config;
mod dialect;

#[cfg(test)]
mod fixture_tests;

pub use adapter::{GeminiAdapter, new_adapter};
pub use config::Config;
pub use dialect::GeminiDialect;
