//! Qdrant [`VectorStore`] implementation and registry integration.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_httpclient::{Auth, HttpClient, HttpClientConfig, Request};
use rskit_vectorstore::{
    Point, PointPayload, SearchFilter, SearchQuery, SearchResult, SimilarityMetric, VectorFactory,
    VectorStore, VectorStoreConfig, VectorStoreLimits, VectorStoreRegistry,
};
use serde::Deserialize;
use serde::de::DeserializeOwned;
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
        let http_config = qdrant_http_config(&config);
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

fn qdrant_http_config(config: &Config) -> HttpClientConfig {
    let mut http_config = HttpClientConfig::new()
        .with_base_url(config.url.trim_end_matches('/'))
        .with_user_agent("rskit-vectorstore-qdrant");
    if let Some(key) = &config.api_key {
        http_config = http_config.with_auth(Auth::api_key_secret("api-key", key.clone()));
    }
    http_config
}

#[async_trait]
impl VectorStore for QdrantVectorStore {
    async fn ensure_collection(&self, collection: &str, dimensions: usize) -> AppResult<()> {
        self.limits.validate_dimensions(dimensions)?;
        let collection_path = qdrant_collection_path(collection)?;
        let response = self
            .client
            .send(Request::get(format!("/collections/{collection_path}")))
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
                Request::put(format!("/collections/{collection_path}")),
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

    async fn upsert(&self, collection: &str, point: Point) -> AppResult<()> {
        let Point {
            id,
            vector,
            payload,
        } = point;
        self.limits.validate_dimensions(vector.len())?;
        let collection_path = qdrant_collection_path(collection)?;
        payload.validate_limits(&self.limits)?;
        debug!(collection, id, "upserting vector point");
        let body = qdrant_upsert_body(&id, vector, payload)?;
        self.send_json(
            Request::put(format!("/collections/{collection_path}/points"))
                .query_param("wait", "true"),
            &body,
            "failed to upsert vector point",
        )
        .await?;
        Ok(())
    }

    async fn search(&self, collection: &str, query: SearchQuery) -> AppResult<Vec<SearchResult>> {
        let SearchQuery {
            vector,
            limit,
            filter,
        } = query;
        self.limits.validate_dimensions(vector.len())?;
        self.limits.validate_search_limit(limit)?;
        if let Some(filter) = filter.as_ref() {
            filter.validate_limits(&self.limits)?;
        }
        let collection_path = qdrant_collection_path(collection)?;

        let body = qdrant_search_body(vector, limit, filter)?;

        let response = self
            .send_json(
                Request::post(format!("/collections/{collection_path}/points/search")),
                &body,
                "vector search failed",
            )
            .await?
            .and_then_qdrant_json::<QdrantSearchResponse>("vector search failed")?;
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
        let collection_path = qdrant_collection_path(collection)?;
        let body = qdrant_delete_body(id)?;
        self.send_json(
            Request::post(format!("/collections/{collection_path}/points/delete"))
                .query_param("wait", "true"),
            &body,
            "failed to delete vector point",
        )
        .await?;
        Ok(())
    }
}

fn qdrant_collection_path(collection: &str) -> AppResult<&str> {
    if collection.is_empty() || matches!(collection, "." | "..") {
        return Err(invalid_qdrant_collection(collection));
    }
    if collection
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        Ok(collection)
    } else {
        Err(invalid_qdrant_collection(collection))
    }
}

fn invalid_qdrant_collection(collection: &str) -> AppError {
    AppError::invalid_input(
        "collection",
        format!("Qdrant collection name must be a non-empty safe URL path segment: {collection:?}"),
    )
}

fn qdrant_upsert_body(id: &str, vector: Vec<f32>, payload: PointPayload) -> AppResult<Value> {
    let payload: HashMap<String, Value> = payload
        .fields
        .into_iter()
        .map(|(k, v)| payload_to_qdrant_value(v).map(|value| (k, value)))
        .collect::<AppResult<_>>()?;
    Ok(serde_json::json!({
        "points": [{
            "id": qdrant_point_id_from_string(id)?,
            "vector": vector,
            "payload": payload,
        }]
    }))
}

fn qdrant_search_body(
    vector: Vec<f32>,
    limit: usize,
    filter: Option<SearchFilter>,
) -> AppResult<Value> {
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
    Ok(body)
}

fn qdrant_delete_body(id: &str) -> AppResult<Value> {
    Ok(serde_json::json!({
        "points": [qdrant_point_id_from_string(id)?],
    }))
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

trait QdrantResponseExt {
    fn and_then_qdrant_json<T: DeserializeOwned>(self, context: &str) -> AppResult<T>;
}

impl QdrantResponseExt for rskit_httpclient::Response {
    fn and_then_qdrant_json<T: DeserializeOwned>(self, context: &str) -> AppResult<T> {
        decode_qdrant_json(self.body_bytes(), context)
    }
}

fn decode_qdrant_json<T: DeserializeOwned>(body: &[u8], context: &str) -> AppResult<T> {
    serde_json::from_slice(body).map_err(|error| {
        AppError::new(
            ErrorCode::ExternalService,
            format!("{context}: failed to decode Qdrant JSON response: {error}"),
        )
        .with_cause(error)
    })
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
    use std::collections::VecDeque;

    use rskit_errors::ErrorCode;
    use rskit_util::SecretString;
    use rskit_vectorstore::{
        PointPayload, SearchFilter, SimilarityMetric, VectorStoreConfig, VectorStoreLimits,
        VectorStoreRegistry,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

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

    #[test]
    fn qdrant_http_config_redacts_api_key() {
        let config = Config {
            url: "https://qdrant.example.test".to_owned(),
            api_key: Some(SecretString::new("super-secret-key")),
            metric: SimilarityMetric::Cosine,
        };

        let debug = format!("{:?}", qdrant_http_config(&config));

        assert!(!debug.contains("super-secret-key"));
        assert!(debug.contains("api-key"));
        assert!(debug.contains("SecretString(***)"));
    }

    #[test]
    fn qdrant_json_decode_errors_are_external_service_failures() {
        let err =
            decode_qdrant_json::<QdrantSearchResponse>(br#"{"result":"not-points"}"#, "search")
                .expect_err("malformed upstream response must fail");

        assert_eq!(err.code(), ErrorCode::ExternalService);
        assert!(
            err.message()
                .contains("failed to decode Qdrant JSON response")
        );
    }

    #[test]
    fn qdrant_collection_path_accepts_safe_single_segments() {
        assert_eq!(
            qdrant_collection_path("tenant_1.collection-prod").unwrap(),
            "tenant_1.collection-prod"
        );
    }

    #[test]
    fn qdrant_collection_path_rejects_unsafe_segments() {
        for collection in [
            "",
            ".",
            "..",
            "tenant/collection",
            "../collection",
            "collection?wait=true",
            "collection#fragment",
            "collection%2fother",
            "collection name",
            "collection\nname",
        ] {
            let err = qdrant_collection_path(collection)
                .expect_err("unsafe collection segment must be rejected");
            assert_eq!(err.code(), ErrorCode::InvalidInput);
        }
    }

    #[test]
    fn qdrant_request_bodies_are_pure_json_mappings() {
        let upsert = qdrant_upsert_body(
            "42",
            vec![0.1, 0.2],
            PointPayload::new()
                .with_field("tag", "blue")
                .with_field("count", 7_i64),
        )
        .unwrap();
        assert_eq!(upsert["points"][0]["id"], 42);
        assert_eq!(upsert["points"][0]["payload"]["tag"], "blue");
        assert_eq!(upsert["points"][0]["payload"]["count"], 7);

        let search = qdrant_search_body(
            vec![0.1, 0.2],
            3,
            Some(SearchFilter::new().must_match("tag", "blue")),
        )
        .unwrap();
        assert_eq!(search["limit"], 3);
        assert_eq!(search["with_payload"], true);
        assert_eq!(
            search["filter"]["must"][0],
            serde_json::json!({"key":"tag","match":{"value":"blue"}})
        );

        let delete = qdrant_delete_body("550e8400-e29b-41d4-a716-446655440000").unwrap();
        assert_eq!(delete["points"][0], "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn qdrant_request_bodies_reject_invalid_ids_before_network() {
        let err = qdrant_upsert_body("bad-id", vec![1.0], PointPayload::new()).unwrap_err();
        assert_eq!(err.code(), ErrorCode::InvalidInput);

        let err = qdrant_delete_body("bad-id").unwrap_err();
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
            .search("test", SearchQuery::new(vec![1.0], 2))
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
            .search("test", SearchQuery::new(vec![1.0], 2))
            .await
            .expect_err("registry Qdrant limit must be enforced before network");

        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn qdrant_ensure_collection_creates_missing_collection() {
        let (base_url, server) = spawn_qdrant_responses(vec![(404, "{}"), (200, "{}")]).await;
        let store = QdrantVectorStore::new(
            Config {
                url: base_url,
                api_key: None,
                metric: SimilarityMetric::Dot,
            },
            VectorStoreLimits::default(),
        )
        .unwrap();

        store.ensure_collection("tenant_vectors", 3).await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn qdrant_ensure_collection_maps_status_errors() {
        let (base_url, server) = spawn_qdrant_responses(vec![(500, "boom")]).await;
        let store = QdrantVectorStore::new(
            Config {
                url: base_url,
                api_key: None,
                metric: SimilarityMetric::Cosine,
            },
            VectorStoreLimits::default(),
        )
        .unwrap();

        let err = store
            .ensure_collection("tenant_vectors", 3)
            .await
            .expect_err("upstream status must be mapped");

        assert_eq!(err.code(), ErrorCode::ExternalService);
        assert!(err.message().contains("failed to check Qdrant collection"));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn qdrant_store_methods_round_trip_against_local_http() {
        let search_body =
            r#"{"result":[{"id":42,"score":0.98,"payload":{"tag":"blue","count":7}}]}"#;
        let (base_url, server) =
            spawn_qdrant_responses(vec![(200, "{}"), (200, search_body), (200, "{}")]).await;
        let store = QdrantVectorStore::new(
            Config {
                url: base_url,
                api_key: Some(SecretString::new("secret")),
                metric: SimilarityMetric::L2,
            },
            VectorStoreLimits::default(),
        )
        .unwrap();

        store
            .upsert(
                "tenant_vectors",
                Point::new(
                    "42",
                    vec![0.1, 0.2],
                    PointPayload::new().with_field("tag", "blue"),
                ),
            )
            .await
            .unwrap();
        let results = store
            .search(
                "tenant_vectors",
                SearchQuery::new(vec![0.1, 0.2], 1)
                    .with_filter(SearchFilter::new().must_match("tag", "blue")),
            )
            .await
            .unwrap();
        store.delete("tenant_vectors", "42").await.unwrap();

        assert_eq!(results[0].id, "42");
        assert_eq!(results[0].score, 0.98);
        assert!(results[0].payload.fields.contains_key("tag"));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn qdrant_search_rejects_unsupported_returned_payload() {
        let search_body =
            r#"{"result":[{"id":42,"score":0.98,"payload":{"nested":{"bad":true}}}]}"#;
        let (base_url, server) = spawn_qdrant_responses(vec![(200, search_body)]).await;
        let store = QdrantVectorStore::new(
            Config {
                url: base_url,
                api_key: None,
                metric: SimilarityMetric::Cosine,
            },
            VectorStoreLimits::default(),
        )
        .unwrap();

        let err = store
            .search("tenant_vectors", SearchQuery::new(vec![0.1, 0.2], 1))
            .await
            .expect_err("unsupported upstream payload must fail");

        assert_eq!(err.code(), ErrorCode::InvalidInput);
        server.await.unwrap();
    }

    async fn spawn_qdrant_responses(
        responses: Vec<(u16, &'static str)>,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let mut responses = VecDeque::from(responses);
            while let Some((status, body)) = responses.pop_front() {
                let (mut socket, _) = listener.accept().await.unwrap();
                tokio::spawn(async move {
                    let mut buffer = [0_u8; 4096];
                    let _ = socket.read(&mut buffer).await;
                    let reason = if status >= 400 { "Error" } else { "OK" };
                    let response = format!(
                        "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    socket.write_all(response.as_bytes()).await.unwrap();
                    socket.shutdown().await.unwrap();
                });
            }
        });
        (format!("http://{address}"), server)
    }
}
