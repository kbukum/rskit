//! Embedding provider abstraction and vector utilities.

mod in_memory;
mod provider;
mod types;

pub use in_memory::InMemoryProvider;
pub use provider::Provider;
pub use rskit_ai::vector::{
    cosine_similarity, dot_product, euclidean_distance, max_pooling, mean_pooling, normalize,
};
pub use rskit_ai::{Model, Usage};
pub use types::{EmbedAsset, EmbedInput, EmbedRequest, EmbedResponse, Embedding};
