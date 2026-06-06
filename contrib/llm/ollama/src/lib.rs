#![warn(missing_docs)]

//! Ollama provider (local/remote LLM via OpenAI-compatible chat completions API).

mod adapter;
mod config;

pub(crate) const PROVIDER_ID: &str = "ollama";

pub use adapter::register;
pub use config::Config;
