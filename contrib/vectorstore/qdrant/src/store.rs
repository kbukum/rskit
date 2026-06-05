//! Qdrant [`VectorStore`] implementation and registry integration.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use qdrant_client::Qdrant;
use qdrant_client::qdrant::{
    CreateCollectionBuilder, DeletePointsBuilder, Filter, PointStruct, SearchPointsBuilder,
    UpsertPointsBuilder, VectorParamsBuilder,
};
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_vectorstore::{
    PointPayload, SearchFilter, SearchResult, SimilarityMetric, VectorFactory, VectorStore,
    VectorStoreConfig, VectorStoreLimits, VectorStoreRegistry,
};
use tracing::{debug, info};

use crate::Config;
use crate::conversion::{
    filter_condition_to_qdrant, payload_to_qdrant_value, qdrant_distance,
    qdrant_point_id_to_string, qdrant_value_to_payload,
};
use crate::url::validate_qdrant_url;

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
            .with_cause(e)
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
                .with_cause(e)
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
                    .with_cause(e)
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
                .with_cause(e)
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
            let conditions = filter
                .must
                .into_iter()
                .map(|condition| filter_condition_to_qdrant(condition.field, condition.equals))
                .collect::<AppResult<Vec<_>>>()?;
            builder = builder.filter(Filter::must(conditions));
        }
        let results = self.client.search_points(builder).await.map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("vector search failed: {e}"),
            )
            .with_cause(e)
        })?;
        results
            .result
            .into_iter()
            .map(|point| {
                let fields = point
                    .payload
                    .into_iter()
                    .map(|(field, value)| {
                        qdrant_value_to_payload(&field, value).map(|value| (field, value))
                    })
                    .collect::<AppResult<_>>()?;

                Ok(SearchResult {
                    id: point
                        .id
                        .ok_or_else(|| {
                            AppError::new(
                                ErrorCode::InvalidInput,
                                "Qdrant search result did not include a point ID",
                            )
                        })
                        .and_then(qdrant_point_id_to_string)?,
                    score: point.score,
                    payload: PointPayload { fields },
                })
            })
            .collect()
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
                .with_cause(e)
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

/// Register the Qdrant vector store factory under the canonical backend name.
pub(crate) fn register_qdrant(registry: &mut VectorStoreRegistry, config: Config) -> AppResult<()> {
    registry.register("qdrant", Arc::new(QdrantFactory { config }))
}

#[cfg(test)]
mod tests {
    use rskit_errors::ErrorCode;
    use rskit_vectorstore::{VectorStoreConfig, VectorStoreLimits, VectorStoreRegistry};

    use super::*;

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
        crate::register(&mut registry, Config::default()).unwrap();

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
