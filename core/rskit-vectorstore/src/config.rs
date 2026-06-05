//! Vector store configuration types.

use serde::{Deserialize, Serialize};

use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::SimilarityMetric;

/// Default maximum number of nearest-neighbor results returned by one search.
pub const DEFAULT_MAX_SEARCH_LIMIT: usize = 1_000;
/// Default maximum vector dimensionality accepted by core stores.
pub const DEFAULT_MAX_VECTOR_DIMENSIONS: usize = 32_768;
/// Default maximum number of scalar payload fields per point.
pub const DEFAULT_MAX_PAYLOAD_FIELDS: usize = 128;
/// Default maximum approximate scalar bytes per point payload or search filter.
pub const DEFAULT_MAX_PAYLOAD_BYTES: usize = 64 * 1024;
/// Default maximum exact-match filter conditions per search.
pub const DEFAULT_MAX_FILTER_CONDITIONS: usize = 32;

/// Config-driven vector store backend selection.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VectorStoreConfig {
    /// Backend name looked up in an injected [`crate::VectorStoreRegistry`].
    #[serde(default = "default_backend")]
    pub backend: String,
    /// In-memory backend options.
    #[serde(default)]
    pub memory: MemoryVectorStoreConfig,
    /// Shared safety limits applied by core stores and adapters.
    #[serde(default)]
    pub limits: VectorStoreLimits,
}

impl Default for VectorStoreConfig {
    fn default() -> Self {
        Self {
            backend: default_backend(),
            memory: MemoryVectorStoreConfig::default(),
            limits: VectorStoreLimits::default(),
        }
    }
}

/// Safety limits for vector operations, untrusted payloads, and filters.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[non_exhaustive]
pub struct VectorStoreLimits {
    /// Maximum `limit` accepted by `search`.
    #[serde(default = "default_max_search_limit")]
    pub max_search_limit: usize,
    /// Maximum vector dimensions accepted for collection/query vectors.
    #[serde(default = "default_max_vector_dimensions")]
    pub max_vector_dimensions: usize,
    /// Maximum scalar fields allowed in a point payload.
    #[serde(default = "default_max_payload_fields")]
    pub max_payload_fields: usize,
    /// Maximum approximate scalar bytes allowed in point payloads and search filters.
    #[serde(default = "default_max_payload_bytes")]
    pub max_payload_bytes: usize,
    /// Maximum exact-match filter conditions accepted by `search`.
    #[serde(default = "default_max_filter_conditions")]
    pub max_filter_conditions: usize,
}

impl VectorStoreLimits {
    /// Create limits with production-safe defaults.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            max_search_limit: DEFAULT_MAX_SEARCH_LIMIT,
            max_vector_dimensions: DEFAULT_MAX_VECTOR_DIMENSIONS,
            max_payload_fields: DEFAULT_MAX_PAYLOAD_FIELDS,
            max_payload_bytes: DEFAULT_MAX_PAYLOAD_BYTES,
            max_filter_conditions: DEFAULT_MAX_FILTER_CONDITIONS,
        }
    }

    /// Override the maximum search result count.
    #[must_use]
    pub const fn with_max_search_limit(mut self, max_search_limit: usize) -> Self {
        self.max_search_limit = max_search_limit;
        self
    }

    /// Override the maximum vector dimensionality.
    #[must_use]
    pub const fn with_max_vector_dimensions(mut self, max_vector_dimensions: usize) -> Self {
        self.max_vector_dimensions = max_vector_dimensions;
        self
    }

    /// Override the maximum number of payload fields.
    #[must_use]
    pub const fn with_max_payload_fields(mut self, max_payload_fields: usize) -> Self {
        self.max_payload_fields = max_payload_fields;
        self
    }

    /// Override the maximum approximate byte count for payloads and filters.
    #[must_use]
    pub const fn with_max_payload_bytes(mut self, max_payload_bytes: usize) -> Self {
        self.max_payload_bytes = max_payload_bytes;
        self
    }

    /// Override the maximum number of exact-match filter conditions.
    #[must_use]
    pub const fn with_max_filter_conditions(mut self, max_filter_conditions: usize) -> Self {
        self.max_filter_conditions = max_filter_conditions;
        self
    }

    /// Validate a vector dimension count against the configured bounds.
    pub fn validate_dimensions(&self, dimensions: usize) -> AppResult<()> {
        if dimensions == 0 || dimensions > self.max_vector_dimensions {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!(
                    "vector dimensions must be between 1 and {}",
                    self.max_vector_dimensions
                ),
            ));
        }
        Ok(())
    }

    /// Validate a search result limit against the configured bounds.
    pub fn validate_search_limit(&self, limit: usize) -> AppResult<()> {
        if limit == 0 || limit > self.max_search_limit {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!(
                    "vector search limit must be between 1 and {}",
                    self.max_search_limit
                ),
            ));
        }
        Ok(())
    }
}

impl Default for VectorStoreLimits {
    fn default() -> Self {
        Self::new()
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

const fn default_max_search_limit() -> usize {
    DEFAULT_MAX_SEARCH_LIMIT
}

const fn default_max_vector_dimensions() -> usize {
    DEFAULT_MAX_VECTOR_DIMENSIONS
}

const fn default_max_payload_fields() -> usize {
    DEFAULT_MAX_PAYLOAD_FIELDS
}

const fn default_max_payload_bytes() -> usize {
    DEFAULT_MAX_PAYLOAD_BYTES
}

const fn default_max_filter_conditions() -> usize {
    DEFAULT_MAX_FILTER_CONDITIONS
}
