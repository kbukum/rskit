//! Vector store trait definition.

use std::collections::HashMap;

use async_trait::async_trait;
use rskit_errors::AppResult;
use serde::{Deserialize, Serialize};

/// Payload stored alongside each vector point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointPayload {
    pub fields: HashMap<String, serde_json::Value>,
}

impl PointPayload {
    pub fn new() -> Self {
        Self {
            fields: HashMap::new(),
        }
    }

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
    pub id: String,
    pub score: f32,
    pub payload: PointPayload,
}

/// Optional filters for search queries.
#[derive(Debug, Clone, Default)]
pub struct SearchFilter {
    /// Filter by exact field match (e.g., platform = "youtube").
    pub must: Vec<(String, serde_json::Value)>,
}

impl SearchFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn must_match(
        mut self,
        field: impl Into<String>,
        value: impl Into<serde_json::Value>,
    ) -> Self {
        self.must.push((field.into(), value.into()));
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
