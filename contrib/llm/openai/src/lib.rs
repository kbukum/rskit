#![warn(missing_docs)]

//! `OpenAI` provider (chat completions + embeddings).

mod adapter;
mod config;
mod embedding;

#[cfg(test)]
mod fixture_tests;
#[cfg(test)]
mod tests;

pub(crate) use adapter::PROVIDER_ID;
pub use adapter::register;
pub use config::Config;
pub use embedding::{EmbeddingProvider, embedding_provider, embedding_provider_with_policy};
