#![allow(missing_docs)]

use rskit_errors::ErrorCode;
use rskit_vectorstore::{
    InMemoryVectorStore, PayloadValue, Point, PointPayload, SearchFilter, SearchQuery,
    SimilarityMetric, VectorStore, VectorStoreConfig, VectorStoreLimits,
};
use serde_json::json;

#[test]
fn payload_values_are_typed_scalars_with_validation_limits() {
    assert_eq!(PayloadValue::from("doc").as_str(), Some("doc"));
    assert_eq!(
        serde_json::to_value(PayloadValue::from(7_i32)).unwrap(),
        json!(7)
    );
    assert_eq!(
        serde_json::from_value::<PayloadValue>(json!(true)).unwrap(),
        PayloadValue::Bool(true)
    );
    assert!(
        serde_json::from_value::<PayloadValue>(serde_json::Value::Number(
            serde_json::Number::from(u64::MAX)
        ))
        .is_err()
    );

    let limits = VectorStoreLimits::new()
        .with_max_payload_fields(1)
        .with_max_payload_bytes(9);
    assert!(
        PointPayload::new()
            .with_field("a", 1_i32)
            .validate_limits(&limits)
            .is_ok()
    );
    assert_eq!(
        PointPayload::new()
            .with_field("a", 1_i32)
            .with_field("b", 2_i32)
            .validate_limits(&limits)
            .unwrap_err()
            .code(),
        ErrorCode::InvalidInput
    );
    assert_eq!(
        PointPayload::new()
            .with_field("long-key", "long-value")
            .validate_limits(&limits)
            .unwrap_err()
            .code(),
        ErrorCode::InvalidInput
    );
    assert_eq!(
        PointPayload::new()
            .with_field("x", f64::NAN)
            .validate_limits(&VectorStoreLimits::new())
            .unwrap_err()
            .code(),
        ErrorCode::InvalidInput
    );
}

#[test]
fn payload_value_conversions_cover_all_scalar_shapes() {
    assert_eq!(
        PayloadValue::from(String::from("owned")),
        PayloadValue::String("owned".into())
    );
    assert_eq!(PayloadValue::from(-7_i64), PayloadValue::Integer(-7));
    assert_eq!(PayloadValue::from(7_u32), PayloadValue::Integer(7));
    assert_eq!(PayloadValue::from(1.25_f64), PayloadValue::Float(1.25));
    assert_eq!(PayloadValue::from(1.5_f32), PayloadValue::Float(1.5));
    assert_eq!(PayloadValue::from(false), PayloadValue::Bool(false));
    assert_eq!(
        serde_json::from_value::<PayloadValue>(json!(-5)).unwrap(),
        PayloadValue::Integer(-5)
    );
    assert_eq!(
        serde_json::from_value::<PayloadValue>(json!(5_u64)).unwrap(),
        PayloadValue::Integer(5)
    );
}

#[test]
fn filters_and_limits_reject_unbounded_search_inputs() {
    let limits = VectorStoreLimits::new()
        .with_max_search_limit(2)
        .with_max_filter_conditions(1)
        .with_max_payload_bytes(8);
    assert!(limits.validate_dimensions(1).is_ok());
    assert_eq!(
        limits.validate_dimensions(0).unwrap_err().code(),
        ErrorCode::InvalidInput
    );
    assert_eq!(
        limits.validate_dimensions(33_000).unwrap_err().code(),
        ErrorCode::InvalidInput
    );
    assert_eq!(
        limits.validate_search_limit(0).unwrap_err().code(),
        ErrorCode::InvalidInput
    );
    assert_eq!(
        limits.validate_search_limit(3).unwrap_err().code(),
        ErrorCode::InvalidInput
    );

    let too_many = SearchFilter::new()
        .must_match("a", 1_i32)
        .must_match("b", 2_i32);
    assert_eq!(
        too_many.validate_limits(&limits).unwrap_err().code(),
        ErrorCode::InvalidInput
    );
    let too_large = SearchFilter::new().must_match("long-key", "long-value");
    assert_eq!(
        too_large.validate_limits(&limits).unwrap_err().code(),
        ErrorCode::InvalidInput
    );
    let non_finite = SearchFilter::new().must_match("x", f64::INFINITY);
    assert_eq!(
        non_finite
            .validate_limits(&VectorStoreLimits::new())
            .unwrap_err()
            .code(),
        ErrorCode::InvalidInput
    );
}

#[test]
fn configs_deserialize_defaults_and_metric_names() {
    let config: VectorStoreConfig =
        serde_json::from_value(json!({"memory":{"metric":"dot"}})).unwrap();
    assert_eq!(config.backend, "memory");
    assert_eq!(config.memory.metric, SimilarityMetric::Dot);
    assert_eq!(
        serde_json::to_value(SimilarityMetric::L2).unwrap(),
        json!("l2")
    );
}

#[tokio::test]
async fn in_memory_store_searches_filters_updates_deletes_and_reports_errors() {
    let store = InMemoryVectorStore::with_metric(SimilarityMetric::Dot);
    store.ensure_collection("docs", 2).await.unwrap();
    store
        .upsert(
            "docs",
            Point::new(
                "a",
                vec![1.0, 0.0],
                PointPayload::new().with_field("kind", "guide"),
            ),
        )
        .await
        .unwrap();
    store
        .upsert(
            "docs",
            Point::new(
                "b",
                vec![0.0, 2.0],
                PointPayload::new().with_field("kind", "api"),
            ),
        )
        .await
        .unwrap();
    store
        .upsert(
            "docs",
            Point::new(
                "a",
                vec![0.0, 3.0],
                PointPayload::new().with_field("kind", "guide"),
            ),
        )
        .await
        .unwrap();

    let filtered = store
        .search(
            "docs",
            SearchQuery::new(vec![0.0, 1.0], 10)
                .with_filter(SearchFilter::new().must_match("kind", "guide")),
        )
        .await
        .unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "a");
    assert!(filtered[0].score > 2.9);

    store.delete("docs", "a").await.unwrap();
    assert!(
        store
            .search("docs", SearchQuery::new(vec![0.0, 1.0], 10))
            .await
            .unwrap()
            .iter()
            .all(|r| r.id != "a")
    );
    assert_eq!(
        store
            .upsert(
                "missing",
                Point::new("x", vec![1.0, 0.0], PointPayload::new())
            )
            .await
            .unwrap_err()
            .code(),
        ErrorCode::NotFound
    );
    assert_eq!(
        store
            .search("docs", SearchQuery::new(vec![1.0], 1))
            .await
            .unwrap_err()
            .code(),
        ErrorCode::InvalidInput
    );
    assert_eq!(
        store.delete("missing", "x").await.unwrap_err().code(),
        ErrorCode::NotFound
    );
}
