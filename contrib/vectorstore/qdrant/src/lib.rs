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
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_httpclient::DestinationPolicy;
use rskit_util::SecretString;
use rskit_vectorstore::{
    PayloadValue, PointPayload, SearchFilter, SearchResult, SimilarityMetric, VectorFactory,
    VectorStore, VectorStoreConfig, VectorStoreLimits, VectorStoreRegistry,
};
use tracing::{debug, info};

/// Configuration for the Qdrant vector store.
#[derive(Clone)]
#[non_exhaustive]
pub struct Config {
    /// Qdrant server URL.
    pub url: String,
    /// Optional API key for Qdrant Cloud.
    pub api_key: Option<SecretString>,
    /// Metric used when creating collections.
    pub metric: SimilarityMetric,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("url", &"<redacted>")
            .field("api_key", &self.api_key)
            .field("metric", &self.metric)
            .finish()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            url: "http://localhost:6334".to_owned(),
            api_key: None,
            metric: SimilarityMetric::Cosine,
        }
    }
}

impl Config {
    /// Create a Qdrant adapter configuration for the given server URL.
    #[must_use]
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            ..Self::default()
        }
    }

    /// Set the optional Qdrant API key.
    #[must_use]
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(SecretString::new(api_key));
        self
    }

    /// Set the collection metric used for newly-created collections.
    #[must_use]
    pub const fn with_metric(mut self, metric: SimilarityMetric) -> Self {
        self.metric = metric;
        self
    }
}

struct QdrantVectorStore {
    client: Qdrant,
    metric: SimilarityMetric,
    limits: VectorStoreLimits,
}

impl QdrantVectorStore {
    fn new(config: Config, limits: VectorStoreLimits) -> AppResult<Self> {
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
            limits,
        })
    }
}

#[async_trait]
impl VectorStore for QdrantVectorStore {
    async fn ensure_collection(&self, collection: &str, dimensions: usize) -> AppResult<()> {
        self.limits.validate_dimensions(dimensions)?;
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
            let distance = qdrant_distance(self.metric)?;
            self.client
                .create_collection(
                    CreateCollectionBuilder::new(collection)
                        .vectors_config(VectorParamsBuilder::new(dimensions as u64, distance)),
                )
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
        self.limits.validate_dimensions(vector.len())?;
        payload.validate_limits(&self.limits)?;
        debug!(collection, id, "upserting vector point");
        let payload: HashMap<String, qdrant_client::qdrant::Value> = payload
            .fields
            .into_iter()
            .map(|(k, v)| payload_to_qdrant_value(v).map(|value| (k, value)))
            .collect::<AppResult<_>>()?;
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
        self.limits.validate_dimensions(vector.len())?;
        self.limits.validate_search_limit(limit)?;
        if let Some(filter) = filter.as_ref() {
            filter.validate_limits(&self.limits)?;
        }
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
                        .filter_map(|(k, v)| qdrant_value_to_payload(v).map(|value| (k, value)))
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
    config: Config,
}

impl VectorFactory for QdrantFactory {
    fn create(&self, config: &VectorStoreConfig) -> AppResult<Arc<dyn VectorStore>> {
        Ok(Arc::new(QdrantVectorStore::new(
            self.config.clone(),
            config.limits,
        )?))
    }
}

/// Explicitly register a configured Qdrant backend.
pub fn register(registry: &mut VectorStoreRegistry, config: Config) -> AppResult<()> {
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
    let parsed = reqwest::Url::parse(url)
        .map_err(|error| AppError::invalid_input("url", format!("invalid Qdrant URL: {error}")))?;
    DestinationPolicy::default().validate(&parsed)
}

fn has_url_credentials(value: &str) -> bool {
    value
        .split_once("://")
        .and_then(|(_, rest)| rest.split('/').next())
        .is_some_and(|authority| authority.contains('@'))
}

fn qdrant_distance(metric: SimilarityMetric) -> AppResult<Distance> {
    match metric {
        SimilarityMetric::Cosine => Ok(Distance::Cosine),
        SimilarityMetric::Dot => Ok(Distance::Dot),
        SimilarityMetric::L2 => Ok(Distance::Euclid),
        _ => Err(AppError::new(
            ErrorCode::InvalidInput,
            format!("unsupported Qdrant similarity metric: {metric:?}"),
        )),
    }
}

fn payload_to_qdrant_value(v: PayloadValue) -> AppResult<qdrant_client::qdrant::Value> {
    use qdrant_client::qdrant::value::Kind;
    let kind = match v {
        PayloadValue::String(s) => Kind::StringValue(s),
        PayloadValue::Integer(n) => Kind::IntegerValue(n),
        PayloadValue::Float(n) => Kind::DoubleValue(n),
        PayloadValue::Bool(b) => Kind::BoolValue(b),
        _ => {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "unsupported vector payload value for Qdrant",
            ));
        }
    };
    Ok(qdrant_client::qdrant::Value { kind: Some(kind) })
}

fn filter_condition_to_qdrant(field: String, value: PayloadValue) -> AppResult<Condition> {
    match value {
        PayloadValue::String(value) => Ok(Condition::matches(field, value)),
        PayloadValue::Integer(value) => Ok(Condition::matches(field, value)),
        PayloadValue::Float(value) => Ok(Condition::range(
            field,
            Range {
                gte: Some(value),
                lte: Some(value),
                ..Default::default()
            },
        )),
        PayloadValue::Bool(value) => Ok(Condition::matches(field, value)),
        _ => Err(AppError::new(
            ErrorCode::InvalidInput,
            "unsupported vector filter value for Qdrant",
        )),
    }
}

fn qdrant_value_to_payload(v: qdrant_client::qdrant::Value) -> Option<PayloadValue> {
    use qdrant_client::qdrant::value::Kind;
    match v.kind {
        Some(Kind::StringValue(s)) => Some(PayloadValue::String(s)),
        Some(Kind::IntegerValue(i)) => Some(PayloadValue::Integer(i)),
        Some(Kind::DoubleValue(d)) => Some(PayloadValue::Float(d)),
        Some(Kind::BoolValue(b)) => Some(PayloadValue::Bool(b)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_connection_details_and_api_key() {
        let config = Config {
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
        assert!(validate_qdrant_url("http://169.254.169.254/latest/meta-data").is_err());
        assert!(validate_qdrant_url("http://[fe80::1]:6334").is_err());
        assert!(validate_qdrant_url("ftp://qdrant.example.test").is_err());
    }

    #[test]
    fn qdrant_distance_maps_supported_metrics() {
        assert_eq!(
            qdrant_distance(SimilarityMetric::Cosine).unwrap(),
            Distance::Cosine
        );
        assert_eq!(
            qdrant_distance(SimilarityMetric::Dot).unwrap(),
            Distance::Dot
        );
        assert_eq!(
            qdrant_distance(SimilarityMetric::L2).unwrap(),
            Distance::Euclid
        );
    }

    #[tokio::test]
    async fn qdrant_store_rejects_unbounded_dimensions_before_network() {
        let store = QdrantVectorStore::new(
            Config::default(),
            VectorStoreLimits::new().with_max_vector_dimensions(2),
        )
        .unwrap();

        let err = store
            .ensure_collection("test", 3)
            .await
            .expect_err("dimension above configured limit must fail");

        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn qdrant_store_rejects_unbounded_search_limit_before_network() {
        let store = QdrantVectorStore::new(
            Config::default(),
            VectorStoreLimits::new().with_max_search_limit(1),
        )
        .unwrap();

        let err = store
            .search("test", vec![1.0], 2, None)
            .await
            .expect_err("search limit above configured limit must fail");

        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn qdrant_uses_registry_limits_when_building_from_registry() {
        let mut registry = VectorStoreRegistry::new();
        register(&mut registry, Config::default()).unwrap();

        let config = VectorStoreConfig {
            backend: "qdrant".to_owned(),
            limits: VectorStoreLimits::new().with_max_search_limit(1),
            ..VectorStoreConfig::default()
        };
        let store = registry.build(&config).unwrap();

        let err = store
            .search("test", vec![1.0], 2, None)
            .await
            .expect_err("registry Qdrant limit must be enforced before network");

        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }
}
