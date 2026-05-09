//! Embedding data types, distance metrics, and aggregation functions.

use serde::{Deserialize, Serialize};

/// Canonical embedding request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedRequest {
    /// Model requested for embedding.
    pub model: rskit_ai::Model,
    /// Inputs to embed.
    pub inputs: Vec<EmbedInput>,
    /// Provider-specific knobs.
    #[serde(default)]
    pub options: serde_json::Value,
}

/// Multimodal embedding input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
#[non_exhaustive]
pub enum EmbedInput {
    /// Text input.
    Text(String),
    /// Image asset.
    Image(EmbedAsset),
    /// Audio asset.
    Audio(EmbedAsset),
    /// Video asset.
    Video(EmbedAsset),
}

/// Bytes or URL asset input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
#[non_exhaustive]
pub enum EmbedAsset {
    /// Inline bytes.
    Bytes(Vec<u8>),
    /// Fetchable URL.
    Url(String),
}

/// Canonical embedding response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedResponse {
    /// Embeddings returned in input order.
    pub embeddings: Vec<Embedding>,
    /// Model that served the request.
    pub model: rskit_ai::Model,
    /// Usage counters.
    pub usage: rskit_ai::Usage,
}

/// A single embedding vector.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Embedding {
    /// The embedding vector.
    pub vector: Vec<f32>,
    /// Vector dimensions.
    pub dimensions: usize,
    /// Zero-based input index.
    pub index: usize,
}

impl Embedding {
    /// Create a new embedding from a vector and input index.
    #[must_use]
    pub fn new(vector: Vec<f32>, index: usize) -> Self {
        let dimensions = vector.len();
        Self {
            vector,
            dimensions,
            index,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedding_sets_dimensions() {
        let e = Embedding::new(vec![1.0, 2.0], 3);
        assert_eq!(e.dimensions, 2);
        assert_eq!(e.index, 3);
    }
}
