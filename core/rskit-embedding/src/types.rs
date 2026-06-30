//! Embedding data types, distance metrics, and aggregation functions.

use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::{Deserialize, Deserializer, Serialize, de};

/// Provider-specific embedding options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct EmbeddingOptions(serde_json::Value);

impl EmbeddingOptions {
    /// Create options from a JSON object.
    pub fn new(value: serde_json::Value) -> AppResult<Self> {
        if value.is_object() {
            Ok(Self(value))
        } else {
            Err(AppError::new(
                ErrorCode::InvalidInput,
                "embedding options must be a JSON object",
            ))
        }
    }

    /// Borrow the structured options.
    #[must_use]
    pub const fn as_json(&self) -> &serde_json::Value {
        &self.0
    }

    /// Consume the wrapper and return structured options.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        self.0
    }
}

impl<'de> Deserialize<'de> for EmbeddingOptions {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(serde_json::Value::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

impl Default for EmbeddingOptions {
    fn default() -> Self {
        Self(serde_json::Value::Object(serde_json::Map::new()))
    }
}

/// Canonical embedding request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedRequest {
    /// Model requested for embedding.
    pub model: rskit_ai::Model,
    /// Inputs to embed.
    pub inputs: Vec<EmbedInput>,
    /// Provider-specific knobs.
    #[serde(default)]
    pub options: EmbeddingOptions,
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
    pub const fn new(vector: Vec<f32>, index: usize) -> Self {
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

    #[test]
    fn embedding_options_reject_non_object() {
        let err = serde_json::from_str::<EmbeddingOptions>("null").unwrap_err();
        assert!(
            err.to_string()
                .contains("embedding options must be a JSON object")
        );
    }
}
