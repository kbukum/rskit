//! OpenAI provider (chat completions + embeddings).

mod adapter;
mod config;
mod dialect;
mod embedding;

pub use adapter::new_adapter;
pub use config::Config;
pub use dialect::OpenAiDialect;
pub use embedding::EmbeddingProvider;
