//! Conversion helpers between rskit vector types and Qdrant API types.

use qdrant_client::qdrant::point_id::PointIdOptions;
use qdrant_client::qdrant::{Condition, Distance, PointId, Range};
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_vectorstore::{PayloadValue, SimilarityMetric};

/// Convert an rskit similarity metric into the equivalent Qdrant distance.
pub(crate) fn qdrant_distance(metric: SimilarityMetric) -> AppResult<Distance> {
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

/// Convert a typed rskit payload value into a Qdrant payload value.
pub(crate) fn payload_to_qdrant_value(v: PayloadValue) -> AppResult<qdrant_client::qdrant::Value> {
    use qdrant_client::qdrant::value::Kind;
    let kind = match v {
        PayloadValue::String(s) => Kind::StringValue(s),
        PayloadValue::Integer(n) => Kind::IntegerValue(n),
        PayloadValue::Float(n) => Kind::DoubleValue(finite_qdrant_float(n, "payload")?),
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

/// Convert an rskit exact-match filter condition into a Qdrant condition.
pub(crate) fn filter_condition_to_qdrant(
    field: String,
    value: PayloadValue,
) -> AppResult<Condition> {
    match value {
        PayloadValue::String(value) => Ok(Condition::matches(field, value)),
        PayloadValue::Integer(value) => Ok(Condition::matches(field, value)),
        PayloadValue::Float(value) => {
            let value = finite_qdrant_float(value, "filter")?;
            Ok(Condition::range(
                field,
                Range {
                    gte: Some(value),
                    lte: Some(value),
                    ..Default::default()
                },
            ))
        }
        PayloadValue::Bool(value) => Ok(Condition::matches(field, value)),
        _ => Err(AppError::new(
            ErrorCode::InvalidInput,
            "unsupported vector filter value for Qdrant",
        )),
    }
}

/// Convert a returned Qdrant payload value into the rskit typed payload contract.
pub(crate) fn qdrant_value_to_payload(
    field: &str,
    v: qdrant_client::qdrant::Value,
) -> AppResult<PayloadValue> {
    use qdrant_client::qdrant::value::Kind;
    match v.kind {
        Some(Kind::StringValue(s)) => Ok(PayloadValue::String(s)),
        Some(Kind::IntegerValue(i)) => Ok(PayloadValue::Integer(i)),
        Some(Kind::DoubleValue(d)) => Ok(PayloadValue::Float(finite_qdrant_float(
            d,
            "returned payload",
        )?)),
        Some(Kind::BoolValue(b)) => Ok(PayloadValue::Bool(b)),
        _ => Err(AppError::new(
            ErrorCode::InvalidInput,
            format!("unsupported Qdrant payload value for field '{field}'"),
        )),
    }
}

/// Convert a returned Qdrant point ID into the stable rskit string ID contract.
pub(crate) fn qdrant_point_id_to_string(id: PointId) -> AppResult<String> {
    match id.point_id_options {
        Some(PointIdOptions::Uuid(value)) => Ok(value),
        Some(PointIdOptions::Num(value)) => Ok(value.to_string()),
        None => Err(AppError::new(
            ErrorCode::InvalidInput,
            "Qdrant search result did not include a point ID",
        )),
    }
}

fn finite_qdrant_float(value: f64, context: &str) -> AppResult<f64> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(AppError::new(
            ErrorCode::InvalidInput,
            format!("Qdrant {context} float values must be finite"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use qdrant_client::qdrant::value::Kind;
    use qdrant_client::qdrant::{Distance, ListValue, PointId, Value};
    use rskit_errors::ErrorCode;
    use rskit_vectorstore::SimilarityMetric;

    use super::*;

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

    #[test]
    fn qdrant_value_to_payload_rejects_unsupported_returned_values() {
        let err = qdrant_value_to_payload(
            "tags",
            Value {
                kind: Some(Kind::ListValue(ListValue { values: Vec::new() })),
            },
        )
        .unwrap_err();

        assert_eq!(err.code(), ErrorCode::InvalidInput);
        assert!(err.message().contains("tags"));
    }

    #[test]
    fn qdrant_payload_conversion_rejects_non_finite_float() {
        let err = payload_to_qdrant_value(PayloadValue::Float(f64::NAN)).unwrap_err();

        assert_eq!(err.code(), ErrorCode::InvalidInput);
        assert!(err.message().contains("finite"));
    }

    #[test]
    fn qdrant_filter_conversion_rejects_non_finite_float() {
        let err =
            filter_condition_to_qdrant("score".to_owned(), PayloadValue::Float(f64::INFINITY))
                .unwrap_err();

        assert_eq!(err.code(), ErrorCode::InvalidInput);
        assert!(err.message().contains("finite"));
    }

    #[test]
    fn qdrant_returned_payload_conversion_rejects_non_finite_float() {
        let err = qdrant_value_to_payload(
            "score",
            Value {
                kind: Some(Kind::DoubleValue(f64::NEG_INFINITY)),
            },
        )
        .unwrap_err();

        assert_eq!(err.code(), ErrorCode::InvalidInput);
        assert!(err.message().contains("finite"));
    }

    #[test]
    fn qdrant_point_id_to_string_uses_stable_variant_value() {
        assert_eq!(
            qdrant_point_id_to_string(PointId {
                point_id_options: Some(PointIdOptions::Uuid("point-1".to_owned())),
            })
            .unwrap(),
            "point-1"
        );
        assert_eq!(
            qdrant_point_id_to_string(PointId {
                point_id_options: Some(PointIdOptions::Num(42)),
            })
            .unwrap(),
            "42"
        );
    }

    #[test]
    fn qdrant_point_id_to_string_rejects_missing_id() {
        let err = qdrant_point_id_to_string(PointId {
            point_id_options: None,
        })
        .unwrap_err();

        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }
}
