//! Ollama provider (local/remote LLM via OpenAI-compatible chat completions API).

mod adapter;
mod config;

pub use adapter::new_adapter;
pub use config::Config;
