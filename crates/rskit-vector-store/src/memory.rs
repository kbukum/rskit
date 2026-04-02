//! In-memory vector store implementation for testing.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use rskit_errors::{AppError, AppResult, ErrorCode};
use tracing::debug;

use crate::store::{PointPayload, SearchFilter, SearchResult, VectorStore};

struct StoredPoint {
    id: String,
    vector: Vec<f32>,
    payload: PointPayload,
}

struct Collection {
    dimensions: usize,
    points: Vec<StoredPoint>,
}

/// In-memory vector store backed by a simple `Vec` with linear scan search.
///
/// Intended for unit tests and prototyping — not suitable for production workloads.
pub struct InMemoryVectorStore {
    collections: Mutex<HashMap<String, Collection>>,
}

impl InMemoryVectorStore {
    /// Create a new empty in-memory vector store.
    pub fn new() -> Self {
        Self {
            collections: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryVectorStore {
    fn default() -> Self {
        Self::new()
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

fn matches_filter(payload: &PointPayload, filter: &SearchFilter) -> bool {
    for (field, expected) in &filter.must {
        match payload.fields.get(field) {
            Some(actual) if actual == expected => {}
            _ => return false,
        }
    }
    true
}

#[async_trait]
impl VectorStore for InMemoryVectorStore {
    async fn ensure_collection(&self, collection: &str, dimensions: usize) -> AppResult<()> {
        let mut collections = self.collections.lock().map_err(|_| {
            AppError::new(ErrorCode::Internal, "in-memory store lock poisoned")
        })?;
        collections
            .entry(collection.to_string())
            .or_insert_with(|| Collection {
                dimensions,
                points: Vec::new(),
            });
        Ok(())
    }

    async fn upsert(
        &self,
        collection: &str,
        id: &str,
        vector: Vec<f32>,
        payload: PointPayload,
    ) -> AppResult<()> {
        debug!(collection, id, "InMemory: upserting vector point");

        let mut collections = self.collections.lock().map_err(|_| {
            AppError::new(ErrorCode::Internal, "in-memory store lock poisoned")
        })?;

        let col = collections.get_mut(collection).ok_or_else(|| {
            AppError::new(
                ErrorCode::NotFound,
                format!("collection '{collection}' does not exist"),
            )
        })?;

        if vector.len() != col.dimensions {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!(
                    "vector dimensions mismatch: expected {}, got {}",
                    col.dimensions,
                    vector.len()
                ),
            ));
        }

        // Update existing or insert new
        if let Some(point) = col.points.iter_mut().find(|p| p.id == id) {
            point.vector = vector;
            point.payload = payload;
        } else {
            col.points.push(StoredPoint {
                id: id.to_string(),
                vector,
                payload,
            });
        }

        Ok(())
    }

    async fn search(
        &self,
        collection: &str,
        vector: Vec<f32>,
        limit: usize,
        filter: Option<SearchFilter>,
    ) -> AppResult<Vec<SearchResult>> {
        debug!(collection, limit, "InMemory: searching vectors");

        let collections = self.collections.lock().map_err(|_| {
            AppError::new(ErrorCode::Internal, "in-memory store lock poisoned")
        })?;

        let col = collections.get(collection).ok_or_else(|| {
            AppError::new(
                ErrorCode::NotFound,
                format!("collection '{collection}' does not exist"),
            )
        })?;

        let mut scored: Vec<SearchResult> = col
            .points
            .iter()
            .filter(|p| {
                filter
                    .as_ref()
                    .map_or(true, |f| matches_filter(&p.payload, f))
            })
            .map(|p| SearchResult {
                id: p.id.clone(),
                score: cosine_similarity(&vector, &p.vector),
                payload: p.payload.clone(),
            })
            .collect();

        // Sort descending by score
        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);

        Ok(scored)
    }

    async fn delete(&self, collection: &str, id: &str) -> AppResult<()> {
        debug!(collection, id, "InMemory: deleting vector point");

        let mut collections = self.collections.lock().map_err(|_| {
            AppError::new(ErrorCode::Internal, "in-memory store lock poisoned")
        })?;

        let col = collections.get_mut(collection).ok_or_else(|| {
            AppError::new(
                ErrorCode::NotFound,
                format!("collection '{collection}' does not exist"),
            )
        })?;

        col.points.retain(|p| p.id != id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ensure_collection_creates_new() {
        let store = InMemoryVectorStore::new();
        store.ensure_collection("test", 3).await.unwrap();
        // Should not error when called again
        store.ensure_collection("test", 3).await.unwrap();
    }

    #[tokio::test]
    async fn test_upsert_and_search() {
        let store = InMemoryVectorStore::new();
        store.ensure_collection("test", 3).await.unwrap();

        let payload = PointPayload::new().with_field("name", "doc1");
        store
            .upsert("test", "1", vec![1.0, 0.0, 0.0], payload)
            .await
            .unwrap();

        let payload = PointPayload::new().with_field("name", "doc2");
        store
            .upsert("test", "2", vec![0.0, 1.0, 0.0], payload)
            .await
            .unwrap();

        let results = store
            .search("test", vec![1.0, 0.0, 0.0], 10, None)
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "1");
        assert!((results[0].score - 1.0).abs() < 1e-6);
    }

    #[tokio::test]
    async fn test_upsert_updates_existing() {
        let store = InMemoryVectorStore::new();
        store.ensure_collection("test", 2).await.unwrap();

        let payload = PointPayload::new().with_field("v", "old");
        store
            .upsert("test", "1", vec![1.0, 0.0], payload)
            .await
            .unwrap();

        let payload = PointPayload::new().with_field("v", "new");
        store
            .upsert("test", "1", vec![0.0, 1.0], payload)
            .await
            .unwrap();

        let results = store
            .search("test", vec![0.0, 1.0], 10, None)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "1");
        assert_eq!(
            results[0].payload.fields.get("v").and_then(|v| v.as_str()),
            Some("new")
        );
    }

    #[tokio::test]
    async fn test_search_with_filter() {
        let store = InMemoryVectorStore::new();
        store.ensure_collection("test", 2).await.unwrap();

        store
            .upsert(
                "test",
                "1",
                vec![1.0, 0.0],
                PointPayload::new().with_field("type", "a"),
            )
            .await
            .unwrap();

        store
            .upsert(
                "test",
                "2",
                vec![1.0, 0.0],
                PointPayload::new().with_field("type", "b"),
            )
            .await
            .unwrap();

        let filter = SearchFilter::new().must_match("type", "a");
        let results = store
            .search("test", vec![1.0, 0.0], 10, Some(filter))
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "1");
    }

    #[tokio::test]
    async fn test_delete() {
        let store = InMemoryVectorStore::new();
        store.ensure_collection("test", 2).await.unwrap();

        store
            .upsert("test", "1", vec![1.0, 0.0], PointPayload::new())
            .await
            .unwrap();

        store.delete("test", "1").await.unwrap();

        let results = store
            .search("test", vec![1.0, 0.0], 10, None)
            .await
            .unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_upsert_wrong_dimensions() {
        let store = InMemoryVectorStore::new();
        store.ensure_collection("test", 3).await.unwrap();

        let result = store
            .upsert("test", "1", vec![1.0, 0.0], PointPayload::new())
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_upsert_missing_collection() {
        let store = InMemoryVectorStore::new();
        let result = store
            .upsert("nonexistent", "1", vec![1.0], PointPayload::new())
            .await;

        assert!(result.is_err());
    }
}
