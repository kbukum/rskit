//! The [`VectorStore`] trait implemented by every backend.

use async_trait::async_trait;
use rskit_errors::AppResult;

use super::point::Point;
use super::query::{SearchQuery, SearchResult};

/// Trait for vector similarity search stores.
#[async_trait]
pub trait VectorStore: Send + Sync {
    /// Ensure a collection exists, creating it if necessary.
    async fn ensure_collection(&self, collection: &str, dimensions: usize) -> AppResult<()>;

    /// Insert or update a vector point.
    async fn upsert(&self, collection: &str, point: Point) -> AppResult<()>;

    /// Search for similar vectors.
    async fn search(&self, collection: &str, query: SearchQuery) -> AppResult<Vec<SearchResult>>;

    /// Delete a point by ID.
    async fn delete(&self, collection: &str, id: &str) -> AppResult<()>;
}
