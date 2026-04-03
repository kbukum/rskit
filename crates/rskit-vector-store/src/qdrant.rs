//! Qdrant implementation of the VectorStore trait.

use std::collections::HashMap;

use async_trait::async_trait;
use qdrant_client::Qdrant;
use qdrant_client::qdrant::{
    Condition, CreateCollectionBuilder, DeletePointsBuilder, Distance, Filter, PointStruct,
    SearchPointsBuilder, UpsertPointsBuilder, VectorParamsBuilder,
};
use rskit_errors::{AppError, AppResult, ErrorCode};
use tracing::{debug, info};

use crate::store::{PointPayload, SearchFilter, SearchResult, VectorStore};

/// Configuration for the Qdrant vector store.
#[derive(Debug, Clone)]
pub struct QdrantConfig {
    /// Qdrant server URL (e.g., `http://localhost:6334`).
    pub url: String,
    /// Optional API key for Qdrant Cloud.
    pub api_key: Option<String>,
}

impl Default for QdrantConfig {
    fn default() -> Self {
        Self {
            url: "http://localhost:6334".to_owned(),
            api_key: None,
        }
    }
}

/// Qdrant-backed vector store.
pub struct QdrantVectorStore {
    client: Qdrant,
}

impl QdrantVectorStore {
    /// Create a new Qdrant vector store from the given configuration.
    pub fn new(config: QdrantConfig) -> AppResult<Self> {
        let mut builder = Qdrant::from_url(&config.url);
        if let Some(key) = &config.api_key {
            builder = builder.api_key(key.clone());
        }
        let client = builder.build().map_err(|e| {
            AppError::new(
                ErrorCode::ConnectionFailed,
                format!("failed to connect to Qdrant: {e}"),
            )
        })?;
        Ok(Self { client })
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
            info!(collection, dimensions, "Creating Qdrant collection");
            self.client
                .create_collection(CreateCollectionBuilder::new(collection).vectors_config(
                    VectorParamsBuilder::new(dimensions as u64, Distance::Cosine),
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
        debug!(collection, id, "Upserting vector point");

        let qdrant_payload: HashMap<String, qdrant_client::qdrant::Value> = payload
            .fields
            .into_iter()
            .map(|(k, v)| (k, json_to_qdrant_value(v)))
            .collect();

        let point = PointStruct::new(id.to_string(), vector, qdrant_payload);

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
        debug!(collection, limit, "Searching vectors");

        let mut builder =
            SearchPointsBuilder::new(collection, vector, limit as u64).with_payload(true);

        if let Some(sf) = filter {
            if !sf.must.is_empty() {
                let conditions: Vec<Condition> = sf
                    .must
                    .into_iter()
                    .filter_map(|(field, value)| {
                        if let Some(s) = value.as_str() {
                            Some(Condition::matches(field, s.to_string()))
                        } else {
                            value.as_i64().map(|n| Condition::matches(field, n))
                        }
                    })
                    .collect();
                builder = builder.filter(Filter::must(conditions));
            }
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
            .map(|point| {
                let payload_fields: HashMap<String, serde_json::Value> = point
                    .payload
                    .into_iter()
                    .map(|(k, v)| (k, qdrant_value_to_json(v)))
                    .collect();

                SearchResult {
                    id: match point.id {
                        Some(id) => format!("{id:?}"),
                        None => String::new(),
                    },
                    score: point.score,
                    payload: PointPayload {
                        fields: payload_fields,
                    },
                }
            })
            .collect())
    }

    async fn delete(&self, collection: &str, id: &str) -> AppResult<()> {
        debug!(collection, id, "Deleting vector point");

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

fn json_to_qdrant_value(v: serde_json::Value) -> qdrant_client::qdrant::Value {
    use qdrant_client::qdrant::value::Kind;
    let kind = match v {
        serde_json::Value::String(s) => Kind::StringValue(s),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Kind::IntegerValue(i)
            } else {
                Kind::DoubleValue(n.as_f64().unwrap_or(0.0))
            }
        }
        serde_json::Value::Bool(b) => Kind::BoolValue(b),
        _ => Kind::StringValue(v.to_string()),
    };
    qdrant_client::qdrant::Value { kind: Some(kind) }
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
    fn test_default_config() {
        let cfg = QdrantConfig::default();
        assert_eq!(cfg.url, "http://localhost:6334");
        assert!(cfg.api_key.is_none());
    }

    #[test]
    fn test_json_to_qdrant_roundtrip() {
        let json_val = serde_json::json!("hello");
        let qdrant_val = json_to_qdrant_value(json_val.clone());
        let back = qdrant_value_to_json(qdrant_val);
        assert_eq!(json_val, back);
    }

    #[test]
    fn test_json_integer_roundtrip() {
        let json_val = serde_json::json!(42);
        let qdrant_val = json_to_qdrant_value(json_val.clone());
        let back = qdrant_value_to_json(qdrant_val);
        assert_eq!(json_val, back);
    }

    #[test]
    fn test_json_bool_roundtrip() {
        let json_val = serde_json::json!(true);
        let qdrant_val = json_to_qdrant_value(json_val.clone());
        let back = qdrant_value_to_json(qdrant_val);
        assert_eq!(json_val, back);
    }
}
