#![warn(missing_docs)]

//! OpenAI provider (chat completions + embeddings).

mod adapter;
mod config;
mod dialect;
mod embedding;

#[cfg(test)]
mod fixture_tests;

pub use adapter::{OpenAiAdapter, new_adapter};
pub use config::Config;
pub use dialect::OpenAiDialect;
pub use embedding::EmbeddingProvider;
