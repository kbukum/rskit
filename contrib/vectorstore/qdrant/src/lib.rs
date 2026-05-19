//! Qdrant adapter for [`rskit_vectorstore`].

#![warn(missing_docs)]

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use qdrant_client::Qdrant;
use qdrant_client::qdrant::{
    Condition, CreateCollectionBuilder, DeletePointsBuilder, Distance, Filter, PointStruct, Range,
    SearchPointsBuilder, UpsertPointsBuilder, VectorParamsBuilder,
};
use rskit_config::SecretString;
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_vectorstore::{
    PointPayload, SearchFilter, SearchResult, SimilarityMetric, VectorFactory, VectorStore,
    VectorStoreConfig, VectorStoreRegistry,
};
use tracing::{debug, info};

/// Configuration for the Qdrant vector store.
#[derive(Clone)]
pub struct QdrantConfig {
    /// Qdrant server URL.
    pub url: String,
    /// Optional API key for Qdrant Cloud.
    pub api_key: Option<SecretString>,
    /// Metric used when creating collections.
    pub metric: SimilarityMetric,
}

impl fmt::Debug for QdrantConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QdrantConfig")
            .field("url", &"<redacted>")
            .field("api_key", &self.api_key)
            .field("metric", &self.metric)
            .finish()
    }
}

impl Default for QdrantConfig {
    fn default() -> Self {
        Self {
            url: "http://localhost:6334".to_owned(),
            api_key: None,
            metric: SimilarityMetric::Cosine,
        }
    }
}

/// Qdrant-backed vector store.
pub struct QdrantVectorStore {
    client: Qdrant,
    metric: SimilarityMetric,
}

impl QdrantVectorStore {
    /// Create a new Qdrant vector store from the given configuration.
    pub fn new(config: QdrantConfig) -> AppResult<Self> {
        validate_qdrant_url(&config.url)?;
        let mut builder = Qdrant::from_url(&config.url);
        if let Some(key) = &config.api_key {
            builder = builder.api_key(key.expose().to_owned());
        }
        let client = builder.build().map_err(|e| {
            AppError::new(
                ErrorCode::ConnectionFailed,
                format!("failed to connect to Qdrant: {e}"),
            )
        })?;
        Ok(Self {
            client,
            metric: config.metric,
        })
    }
}

#[async_trait]
impl VectorStore for QdrantVectorStore {
    async fn ensure_collection(&self, collection: &str, dimensions: usize) -> AppResult<()> {
        let exists = self
            .client
            .collection_exists(collection)
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::ExternalService,
                    format!("failed to check Qdrant collection: {e}"),
                )
            })?;
        if !exists {
            info!(collection, dimensions, "creating Qdrant collection");
            self.client
                .create_collection(CreateCollectionBuilder::new(collection).vectors_config(
                    VectorParamsBuilder::new(dimensions as u64, qdrant_distance(self.metric)),
                ))
                .await
                .map_err(|e| {
                    AppError::new(
                        ErrorCode::ExternalService,
                        format!("failed to create Qdrant collection: {e}"),
                    )
                })?;
        }
        Ok(())
    }

    async fn upsert(
        &self,
        collection: &str,
        id: &str,
        vector: Vec<f32>,
        payload: PointPayload,
    ) -> AppResult<()> {
        debug!(collection, id, "upserting vector point");
        let payload: HashMap<String, qdrant_client::qdrant::Value> = payload
            .fields
            .into_iter()
            .map(|(k, v)| (k, json_to_qdrant_value(v)))
            .collect();
        let point = PointStruct::new(id.to_string(), vector, payload);
        self.client
            .upsert_points(UpsertPointsBuilder::new(collection, vec![point]).wait(true))
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::ExternalService,
                    format!("failed to upsert vector point: {e}"),
                )
            })?;
        Ok(())
    }

    async fn search(
        &self,
        collection: &str,
        vector: Vec<f32>,
        limit: usize,
        filter: Option<SearchFilter>,
    ) -> AppResult<Vec<SearchResult>> {
        let mut builder =
            SearchPointsBuilder::new(collection, vector, limit as u64).with_payload(true);
        if let Some(filter) = filter
            && !filter.must.is_empty()
        {
            let conditions: AppResult<Vec<Condition>> = filter
                .must
                .into_iter()
                .map(|condition| filter_condition_to_qdrant(condition.field, condition.equals))
                .collect();
            builder = builder.filter(Filter::must(conditions?));
        }
        let results = self.client.search_points(builder).await.map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("vector search failed: {e}"),
            )
        })?;
        Ok(results
            .result
            .into_iter()
            .map(|point| SearchResult {
                id: point.id.map_or_else(String::new, |id| format!("{id:?}")),
                score: point.score,
                payload: PointPayload {
                    fields: point
                        .payload
                        .into_iter()
                        .map(|(k, v)| (k, qdrant_value_to_json(v)))
                        .collect(),
                },
            })
            .collect())
    }

    async fn delete(&self, collection: &str, id: &str) -> AppResult<()> {
        use qdrant_client::qdrant::PointId;
        use qdrant_client::qdrant::point_id::PointIdOptions;

        let point_id = PointId {
            point_id_options: Some(PointIdOptions::Uuid(id.to_string())),
        };
        self.client
            .delete_points(
                DeletePointsBuilder::new(collection)
                    .points(vec![point_id])
                    .wait(true),
            )
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::ExternalService,
                    format!("failed to delete vector point: {e}"),
                )
            })?;
        Ok(())
    }
}

struct QdrantFactory {
    config: QdrantConfig,
}

impl VectorFactory for QdrantFactory {
    fn create(&self, _config: &VectorStoreConfig) -> AppResult<Arc<dyn VectorStore>> {
        Ok(Arc::new(QdrantVectorStore::new(self.config.clone())?))
    }
}

/// Explicitly register a configured Qdrant backend.
pub fn register_qdrant(registry: &mut VectorStoreRegistry, config: QdrantConfig) -> AppResult<()> {
    registry.register("qdrant", Arc::new(QdrantFactory { config }))
}

fn validate_qdrant_url(url: &str) -> AppResult<()> {
    if url.trim().is_empty() {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "Qdrant URL is required",
        ));
    }
    if has_url_credentials(url) || url.contains('?') {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "Qdrant URL must not contain credentials or query parameters",
        ));
    }
    Ok(())
}

fn has_url_credentials(value: &str) -> bool {
    value
        .split_once("://")
        .and_then(|(_, rest)| rest.split('/').next())
        .is_some_and(|authority| authority.contains('@'))
}

fn qdrant_distance(metric: SimilarityMetric) -> Distance {
    match metric {
        SimilarityMetric::Cosine => Distance::Cosine,
        SimilarityMetric::Dot => Distance::Dot,
        SimilarityMetric::L2 => Distance::Euclid,
        _ => Distance::Cosine,
    }
}

fn json_to_qdrant_value(v: serde_json::Value) -> qdrant_client::qdrant::Value {
    use qdrant_client::qdrant::value::Kind;
    let kind = match v {
        serde_json::Value::String(s) => Kind::StringValue(s),
        serde_json::Value::Number(n) => n.as_i64().map_or_else(
            || Kind::DoubleValue(n.as_f64().unwrap_or(0.0)),
            Kind::IntegerValue,
        ),
        serde_json::Value::Bool(b) => Kind::BoolValue(b),
        _ => Kind::StringValue(v.to_string()),
    };
    qdrant_client::qdrant::Value { kind: Some(kind) }
}

fn filter_condition_to_qdrant(field: String, value: serde_json::Value) -> AppResult<Condition> {
    if let Some(s) = value.as_str() {
        return Ok(Condition::matches(field, s.to_string()));
    }
    if let Some(n) = value.as_i64() {
        return Ok(Condition::matches(field, n));
    }
    if let Some(n) = value.as_u64() {
        let signed = i64::try_from(n).map_err(|_| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("unsupported Qdrant unsigned integer filter value for field '{field}'"),
            )
        })?;
        return Ok(Condition::matches(field, signed));
    }
    if let Some(n) = value.as_f64() {
        return Ok(Condition::range(
            field,
            Range {
                gte: Some(n),
                lte: Some(n),
                ..Default::default()
            },
        ));
    }
    if let Some(b) = value.as_bool() {
        return Ok(Condition::matches(field, b));
    }
    Err(AppError::new(
        ErrorCode::InvalidInput,
        "unsupported Qdrant filter value",
    ))
}

fn qdrant_value_to_json(v: qdrant_client::qdrant::Value) -> serde_json::Value {
    use qdrant_client::qdrant::value::Kind;
    match v.kind {
        Some(Kind::StringValue(s)) => serde_json::Value::String(s),
        Some(Kind::IntegerValue(i)) => serde_json::json!(i),
        Some(Kind::DoubleValue(d)) => serde_json::json!(d),
        Some(Kind::BoolValue(b)) => serde_json::Value::Bool(b),
        _ => serde_json::Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_connection_details_and_api_key() {
        let config = QdrantConfig {
            url: "https://qdrant.example.test:6334".to_owned(),
            api_key: Some(SecretString::new("super-secret")),
            metric: SimilarityMetric::Cosine,
        };

        let debug = format!("{config:?}");

        assert!(!debug.contains("qdrant.example.test"));
        assert!(!debug.contains("super-secret"));
        assert!(debug.contains("<redacted>"));
        assert!(debug.contains("SecretString(***)"));
    }

    #[test]
    fn rejects_sensitive_url_forms() {
        assert!(validate_qdrant_url("https://user:pass@qdrant.example.test").is_err());
        assert!(validate_qdrant_url("https://qdrant.example.test?api_key=secret").is_err());
    }
}
