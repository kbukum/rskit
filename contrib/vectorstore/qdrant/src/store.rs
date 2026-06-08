//! Qdrant [`VectorStore`] implementation and registry integration.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_httpclient::{HttpClient, HttpClientConfig, Request};
use rskit_vectorstore::{
    PointPayload, SearchFilter, SearchResult, SimilarityMetric, VectorFactory, VectorStore,
    VectorStoreConfig, VectorStoreLimits, VectorStoreRegistry,
};
use serde::Deserialize;
use serde_json::Value;
use tracing::{debug, info};

use crate::Config;
use crate::conversion::{
    QdrantPointId, filter_condition_to_qdrant, payload_to_qdrant_value, qdrant_distance,
    qdrant_point_id_from_string, qdrant_point_id_to_string, qdrant_value_to_payload,
};
use crate::url::validate_qdrant_url;

struct QdrantVectorStore {
    client: HttpClient,
    metric: SimilarityMetric,
    limits: VectorStoreLimits,
}

impl QdrantVectorStore {
    fn new(config: Config, limits: VectorStoreLimits) -> AppResult<Self> {
        validate_qdrant_url(&config.url)?;
        let mut http_config = HttpClientConfig::new()
            .with_base_url(config.url.trim_end_matches('/'))
            .with_user_agent("rskit-vectorstore-qdrant");
        if let Some(key) = &config.api_key {
            http_config = http_config.with_header("api-key", key.expose());
        }
        Ok(Self {
            client: HttpClient::new(http_config)?,
            metric: config.metric,
            limits,
        })
    }

    async fn send_json<T: serde::Serialize>(
        &self,
        request: Request,
        body: &T,
        context: &str,
    ) -> AppResult<rskit_httpclient::Response> {
        self.client
            .send(request.json_body(body).map_err(|error| {
                AppError::new(
                    ErrorCode::InvalidInput,
                    format!("failed to encode Qdrant request body: {error}"),
                )
                .with_cause(error)
            })?)
            .await?
            .error_for_status_with(|response| qdrant_http_error(context, response))
    }
}

#[async_trait]
impl VectorStore for QdrantVectorStore {
    async fn ensure_collection(&self, collection: &str, dimensions: usize) -> AppResult<()> {
        self.limits.validate_dimensions(dimensions)?;
        let response = self
            .client
            .send(Request::get(format!("/collections/{collection}")))
            .await?;
        if response.status_u16() == 404 {
            info!(collection, dimensions, "creating Qdrant collection");
            let body = serde_json::json!({
                "vectors": {
                    "size": dimensions,
                    "distance": qdrant_distance(self.metric)?,
                }
            });
            self.send_json(
                Request::put(format!("/collections/{collection}")),
                &body,
                "failed to create Qdrant collection",
            )
            .await?;
            return Ok(());
        }
        response.error_for_status_with(|response| {
            qdrant_http_error("failed to check Qdrant collection", response)
        })?;
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
        let payload: HashMap<String, Value> = payload
            .fields
            .into_iter()
            .map(|(k, v)| payload_to_qdrant_value(v).map(|value| (k, value)))
            .collect::<AppResult<_>>()?;
        let body = serde_json::json!({
            "points": [{
                "id": qdrant_point_id_from_string(id)?,
                "vector": vector,
                "payload": payload,
            }]
        });
        self.send_json(
            Request::put(format!("/collections/{collection}/points")).query_param("wait", "true"),
            &body,
            "failed to upsert vector point",
        )
        .await?;
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

        let mut body = serde_json::json!({
            "vector": vector,
            "limit": limit,
            "with_payload": true,
        });
        if let Some(filter) = filter
            && !filter.must.is_empty()
        {
            let conditions = filter
                .must
                .into_iter()
                .map(|condition| filter_condition_to_qdrant(condition.field, condition.equals))
                .collect::<AppResult<Vec<_>>>()?;
            body["filter"] = serde_json::json!({ "must": conditions });
        }

        let response = self
            .send_json(
                Request::post(format!("/collections/{collection}/points/search")),
                &body,
                "vector search failed",
            )
            .await?
            .json::<QdrantSearchResponse>()?;
        response
            .result
            .into_iter()
            .map(|point| {
                let fields = point
                    .payload
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(field, value)| {
                        qdrant_value_to_payload(&field, value).map(|value| (field, value))
                    })
                    .collect::<AppResult<_>>()?;

                Ok(SearchResult {
                    id: qdrant_point_id_to_string(point.id),
                    score: point.score,
                    payload: PointPayload { fields },
                })
            })
            .collect()
    }

    async fn delete(&self, collection: &str, id: &str) -> AppResult<()> {
        let body = serde_json::json!({
            "points": [qdrant_point_id_from_string(id)?],
        });
        self.send_json(
            Request::post(format!("/collections/{collection}/points/delete"))
                .query_param("wait", "true"),
            &body,
            "failed to delete vector point",
        )
        .await?;
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct QdrantSearchResponse {
    result: Vec<QdrantScoredPoint>,
}

#[derive(Debug, Deserialize)]
struct QdrantScoredPoint {
    id: QdrantPointId,
    score: f32,
    payload: Option<HashMap<String, Value>>,
}

fn qdrant_http_error(context: &str, response: rskit_httpclient::ErrorResponse) -> AppError {
    AppError::new(
        ErrorCode::ExternalService,
        format!("{context}: Qdrant returned HTTP {}", response.status),
    )
    .with_detail("body", response.body)
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
