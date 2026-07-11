//! In-memory vector store implementation for testing.

use std::collections::HashMap;

use async_trait::async_trait;
use parking_lot::Mutex as ParkingMutex;
use rskit_errors::{AppError, AppResult, ErrorCode};
use tracing::debug;

use crate::config::VectorStoreLimits;
use crate::store::{PointPayload, SearchFilter, SearchResult, SimilarityMetric, VectorStore};

struct StoredPoint {
    id: String,
    vector: Vec<f32>,
    payload: PointPayload,
}

struct Collection {
    dimensions: usize,
    metric: SimilarityMetric,
    points: Vec<StoredPoint>,
}

/// In-memory vector store backed by a simple `Vec` with linear scan search.
///
/// Intended for unit tests and prototyping — not suitable for production workloads.
pub struct InMemoryVectorStore {
    default_metric: SimilarityMetric,
    limits: VectorStoreLimits,
    collections: ParkingMutex<HashMap<String, Collection>>,
}

impl InMemoryVectorStore {
    /// Create a new empty in-memory vector store.
    pub fn new() -> Self {
        Self::with_metric(SimilarityMetric::Cosine)
    }

    /// Create a new empty in-memory vector store with the metric used for new collections.
    #[must_use]
    pub fn with_metric(default_metric: SimilarityMetric) -> Self {
        Self::with_options(default_metric, VectorStoreLimits::default())
    }

    /// Create a new empty in-memory vector store with explicit safety limits.
    #[must_use]
    pub fn with_options(default_metric: SimilarityMetric, limits: VectorStoreLimits) -> Self {
        Self {
            default_metric,
            limits,
            collections: ParkingMutex::new(HashMap::new()),
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

fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

fn l2_score(a: &[f32], b: &[f32]) -> f32 {
    -a.iter()
        .zip(b.iter())
        .map(|(x, y)| {
            let delta = x - y;
            delta * delta
        })
        .sum::<f32>()
        .sqrt()
}

fn similarity_score(metric: SimilarityMetric, a: &[f32], b: &[f32]) -> f32 {
    match metric {
        SimilarityMetric::Cosine => cosine_similarity(a, b),
        SimilarityMetric::Dot => dot_product(a, b),
        SimilarityMetric::L2 => l2_score(a, b),
    }
}

fn matches_filter(payload: &PointPayload, filter: &SearchFilter) -> bool {
    for condition in &filter.must {
        match payload.fields.get(&condition.field) {
            Some(actual) if actual == &condition.equals => {}
            _ => return false,
        }
    }
    true
}

fn compare_score_desc(a: f32, b: f32) -> std::cmp::Ordering {
    b.partial_cmp(&a).unwrap_or(std::cmp::Ordering::Equal)
}

#[async_trait]
impl VectorStore for InMemoryVectorStore {
    async fn ensure_collection(&self, collection: &str, dimensions: usize) -> AppResult<()> {
        self.limits.validate_dimensions(dimensions)?;
        let mut collections = self.collections.lock();
        collections
            .entry(collection.to_string())
            .or_insert_with(|| Collection {
                dimensions,
                metric: self.default_metric,
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

        self.limits.validate_dimensions(vector.len())?;
        payload.validate_limits(&self.limits)?;

        let mut collections = self.collections.lock();

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

        validate_search(limit, filter.as_ref(), &self.limits)?;
        self.limits.validate_dimensions(vector.len())?;

        let collections = self.collections.lock();

        let col = collections.get(collection).ok_or_else(|| {
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

        fn validate_search(
            limit: usize,
            filter: Option<&SearchFilter>,
            limits: &VectorStoreLimits,
        ) -> AppResult<()> {
            limits.validate_search_limit(limit)?;
            if let Some(filter) = filter {
                filter.validate_limits(limits)?;
            }
            Ok(())
        }

        let mut scored: Vec<SearchResult> = col
            .points
            .iter()
            .filter(|p| {
                filter
                    .as_ref()
                    .is_none_or(|f| matches_filter(&p.payload, f))
            })
            .map(|p| SearchResult {
                id: p.id.clone(),
                score: similarity_score(col.metric, &vector, &p.vector),
                payload: p.payload.clone(),
            })
            .collect();

        scored.sort_by(|a, b| compare_score_desc(a.score, b.score));
        scored.truncate(limit);

        Ok(scored)
    }

    async fn delete(&self, collection: &str, id: &str) -> AppResult<()> {
        debug!(collection, id, "InMemory: deleting vector point");

        let mut collections = self.collections.lock();

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

    #[test]
    fn similarity_helpers_cover_all_metrics_and_zero_vectors() {
        let default_store = InMemoryVectorStore::default();
        assert_eq!(default_store.default_metric, SimilarityMetric::Cosine);
        assert_eq!(cosine_similarity(&[0.0, 0.0], &[1.0, 0.0]), 0.0);
        assert_eq!(dot_product(&[1.0, 2.0], &[3.0, 4.0]), 11.0);
        assert_eq!(l2_score(&[1.0, 1.0], &[4.0, 5.0]), -5.0);
        assert_eq!(
            similarity_score(SimilarityMetric::L2, &[1.0, 1.0], &[4.0, 5.0]),
            -5.0
        );
    }

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
    async fn configured_metric_is_used_for_new_collections() {
        let store = InMemoryVectorStore::with_metric(SimilarityMetric::Dot);
        store.ensure_collection("test", 2).await.unwrap();

        store
            .upsert("test", "long", vec![10.0, 0.0], PointPayload::new())
            .await
            .unwrap();
        store
            .upsert("test", "unit", vec![1.0, 0.0], PointPayload::new())
            .await
            .unwrap();

        let results = store
            .search("test", vec![1.0, 0.0], 10, None)
            .await
            .unwrap();

        assert_eq!(results[0].id, "long");
        assert_eq!(results[0].score, 10.0);
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
    async fn test_search_wrong_dimensions_returns_invalid_input() {
        let store = InMemoryVectorStore::new();
        store.ensure_collection("test", 3).await.unwrap();

        let err = store
            .search("test", vec![1.0, 0.0], 10, None)
            .await
            .expect_err("dimension mismatch must fail");

        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn search_rejects_limit_above_configured_bound() {
        let store = InMemoryVectorStore::with_options(
            SimilarityMetric::Cosine,
            VectorStoreLimits::new().with_max_search_limit(1),
        );
        store.ensure_collection("test", 2).await.unwrap();

        let err = store
            .search("test", vec![1.0, 0.0], 2, None)
            .await
            .expect_err("search limit above configured bound must fail");

        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn upsert_rejects_payload_above_configured_bound() {
        let store = InMemoryVectorStore::with_options(
            SimilarityMetric::Cosine,
            VectorStoreLimits::new().with_max_payload_bytes(4),
        );
        store.ensure_collection("test", 2).await.unwrap();

        let err = store
            .upsert(
                "test",
                "1",
                vec![1.0, 0.0],
                PointPayload::new().with_field("name", "too-large"),
            )
            .await
            .expect_err("payload above configured bound must fail");

        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn search_rejects_filter_above_configured_bound() {
        let store = InMemoryVectorStore::with_options(
            SimilarityMetric::Cosine,
            VectorStoreLimits::new().with_max_payload_bytes(4),
        );
        store.ensure_collection("test", 2).await.unwrap();

        let filter = SearchFilter::new().must_match("name", "too-large");
        let err = store
            .search("test", vec![1.0, 0.0], 1, Some(filter))
            .await
            .expect_err("filter above configured bound must fail");

        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn search_rejects_non_finite_filter_float() {
        let store = InMemoryVectorStore::new();
        store.ensure_collection("test", 2).await.unwrap();

        let filter = SearchFilter::new().must_match("score", f64::NAN);
        let err = store
            .search("test", vec![1.0, 0.0], 1, Some(filter))
            .await
            .expect_err("non-finite filter float must fail");

        assert_eq!(err.code(), ErrorCode::InvalidInput);
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
