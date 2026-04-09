//! Embedding provider abstraction and vector utilities.

mod provider;
mod types;

pub use provider::EmbeddingProvider;
pub use types::{
    Embedding, cosine_similarity, dot_product, euclidean_distance, max_pooling, mean_pooling,
};
