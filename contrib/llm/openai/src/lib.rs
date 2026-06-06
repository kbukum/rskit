#![warn(missing_docs)]

//! `OpenAI` provider (chat completions + embeddings).

mod adapter;
mod config;
mod embedding;

pub(crate) const PROVIDER_ID: &str = "openai";

#[cfg(test)]
mod fixture_tests;

pub use adapter::register;
pub use config::Config;
pub use embedding::EmbeddingProvider;

/// Build an OpenAI-compatible embedding provider from adapter configuration.
pub fn embedding_provider(config: &Config) -> rskit_errors::AppResult<EmbeddingProvider> {
    embedding::EmbeddingProvider::new(config)
}

/// Build an OpenAI-compatible embedding provider with a resilience policy.
pub fn embedding_provider_with_policy(
    config: &Config,
    policy: rskit_resilience::Policy,
) -> rskit_errors::AppResult<EmbeddingProvider> {
    Ok(embedding::EmbeddingProvider::new(config)?.with_policy(policy))
}
