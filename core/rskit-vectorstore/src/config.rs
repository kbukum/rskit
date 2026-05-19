//! Vector store configuration types.

use serde::{Deserialize, Serialize};

use crate::SimilarityMetric;

/// Config-driven vector store backend selection.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VectorStoreConfig {
    /// Backend name looked up in an injected [`crate::VectorStoreRegistry`].
    #[serde(default = "default_backend")]
    pub backend: String,
    /// In-memory backend options.
    #[serde(default)]
    pub memory: MemoryVectorStoreConfig,
}

impl Default for VectorStoreConfig {
    fn default() -> Self {
        Self {
            backend: default_backend(),
            memory: MemoryVectorStoreConfig::default(),
        }
    }
}

/// In-memory vector store options.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryVectorStoreConfig {
    /// Default metric used for collections created by the memory backend.
    #[serde(default)]
    pub metric: SimilarityMetric,
}

impl Default for MemoryVectorStoreConfig {
    fn default() -> Self {
        Self {
            metric: SimilarityMetric::Cosine,
        }
    }
}

fn default_backend() -> String {
    "memory".to_owned()
}
