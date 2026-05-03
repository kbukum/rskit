//! Vector store trait definition.

use std::collections::HashMap;

use async_trait::async_trait;
use rskit_errors::AppResult;
use serde::{Deserialize, Serialize};

/// Payload stored alongside each vector point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointPayload {
    /// Metadata fields carried with the point.
    pub fields: HashMap<String, serde_json::Value>,
}

impl PointPayload {
    /// Create an empty payload.
    #[must_use]
    pub fn new() -> Self {
        Self {
            fields: HashMap::new(),
        }
    }

    /// Add a payload field.
    #[must_use]
    pub fn with_field(
        mut self,
        key: impl Into<String>,
        value: impl Into<serde_json::Value>,
    ) -> Self {
        self.fields.insert(key.into(), value.into());
        self
    }
}

impl Default for PointPayload {
    fn default() -> Self {
        Self::new()
    }
}

/// A single search result from the vector store.
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// Point identifier.
    pub id: String,
    /// Backend-specific similarity score.
    pub score: f32,
    /// Payload attached to the point.
    pub payload: PointPayload,
}

/// Canonical vector distance/similarity metrics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "lowercase")]
pub enum SimilarityMetric {
    /// Cosine similarity.
    #[default]
    Cosine,
    /// Dot product.
    Dot,
    /// Euclidean L2 distance.
    L2,
}

/// Exact-match metadata filter condition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FilterCondition {
    /// Payload field path.
    pub field: String,
    /// Exact value to match.
    pub equals: serde_json::Value,
}

/// Optional filters for search queries.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchFilter {
    /// Filter by exact field match (e.g., platform = "youtube").
    pub must: Vec<FilterCondition>,
}

impl SearchFilter {
    /// Create an empty filter.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an exact-match condition to the `must` list.
    #[must_use]
    pub fn must_match(
        mut self,
        field: impl Into<String>,
        value: impl Into<serde_json::Value>,
    ) -> Self {
        self.must.push(FilterCondition {
            field: field.into(),
            equals: value.into(),
        });
        self
    }
}

/// Trait for vector similarity search stores.
#[async_trait]
pub trait VectorStore: Send + Sync {
    /// Ensure a collection exists, creating it if necessary.
    async fn ensure_collection(&self, collection: &str, dimensions: usize) -> AppResult<()>;

    /// Insert or update a vector point.
    async fn upsert(
        &self,
        collection: &str,
        id: &str,
        vector: Vec<f32>,
        payload: PointPayload,
    ) -> AppResult<()>;

    /// Search for similar vectors.
    async fn search(
        &self,
        collection: &str,
        vector: Vec<f32>,
        limit: usize,
        filter: Option<SearchFilter>,
    ) -> AppResult<Vec<SearchResult>>;

    /// Delete a point by ID.
    async fn delete(&self, collection: &str, id: &str) -> AppResult<()>;
}
