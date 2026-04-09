//! Google Gemini provider (Generative Language API).

mod adapter;
mod config;
mod dialect;

pub use adapter::new_adapter;
pub use config::Config;
pub use dialect::GeminiDialect;
