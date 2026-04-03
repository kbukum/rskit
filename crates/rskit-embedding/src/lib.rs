//! Embedding provider abstraction and OpenAI-compatible implementation.

mod openai;
mod provider;
mod types;

pub use openai::{OpenAiEmbeddingConfig, OpenAiEmbeddingProvider};
pub use provider::EmbeddingProvider;
pub use types::{
    Embedding, cosine_similarity, dot_product, euclidean_distance, max_pooling, mean_pooling,
};
