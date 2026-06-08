//! Conversion helpers between rskit vector types and Qdrant REST payloads.

use serde::{Deserialize, Serialize};
use serde_json::{Number, Value};

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_vectorstore::{PayloadValue, SimilarityMetric};

/// Qdrant point identifier accepted by the REST API.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub(crate) enum QdrantPointId {
    /// Numeric point identifier.
    Num(u64),
    /// UUID point identifier.
    Uuid(String),
}

/// Convert an rskit similarity metric into Qdrant's REST distance string.
pub(crate) fn qdrant_distance(metric: SimilarityMetric) -> AppResult<&'static str> {
    match metric {
        SimilarityMetric::Cosine => Ok("Cosine"),
        SimilarityMetric::Dot => Ok("Dot"),
        SimilarityMetric::L2 => Ok("Euclid"),
        _ => Err(AppError::new(
            ErrorCode::InvalidInput,
            format!("unsupported Qdrant similarity metric: {metric:?}"),
        )),
    }
}

/// Convert a typed rskit payload value into a Qdrant JSON payload value.
pub(crate) fn payload_to_qdrant_value(v: PayloadValue) -> AppResult<Value> {
    match v {
        PayloadValue::String(s) => Ok(Value::String(s)),
        PayloadValue::Integer(n) => Ok(Value::Number(n.into())),
        PayloadValue::Float(n) => Number::from_f64(finite_qdrant_float(n, "payload")?)
            .map(Value::Number)
            .ok_or_else(|| {
                AppError::new(ErrorCode::InvalidInput, "Qdrant payload float is invalid")
            }),
        PayloadValue::Bool(b) => Ok(Value::Bool(b)),
        _ => Err(AppError::new(
            ErrorCode::InvalidInput,
            "unsupported vector payload value for Qdrant",
        )),
    }
}

/// Convert an rskit exact-match filter condition into Qdrant REST JSON.
pub(crate) fn filter_condition_to_qdrant(field: String, value: PayloadValue) -> AppResult<Value> {
    let condition = match value {
        PayloadValue::String(value) => {
            serde_json::json!({ "key": field, "match": { "value": value } })
        }
        PayloadValue::Integer(value) => {
            serde_json::json!({ "key": field, "match": { "value": value } })
        }
        PayloadValue::Float(value) => {
            let value = finite_qdrant_float(value, "filter")?;
            serde_json::json!({ "key": field, "range": { "gte": value, "lte": value } })
        }
        PayloadValue::Bool(value) => {
            serde_json::json!({ "key": field, "match": { "value": value } })
        }
        _ => {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "unsupported vector filter value for Qdrant",
            ));
        }
    };
    Ok(condition)
}

/// Convert a returned Qdrant JSON payload value into the rskit typed payload contract.
pub(crate) fn qdrant_value_to_payload(field: &str, value: Value) -> AppResult<PayloadValue> {
    match value {
        Value::String(s) => Ok(PayloadValue::String(s)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(PayloadValue::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(PayloadValue::Float(finite_qdrant_float(
                    f,
                    "returned payload",
                )?))
            } else {
                Err(unsupported_payload(field))
            }
        }
        Value::Bool(b) => Ok(PayloadValue::Bool(b)),
        _ => Err(unsupported_payload(field)),
    }
}

/// Convert a returned Qdrant point ID into the stable rskit string ID contract.
pub(crate) fn qdrant_point_id_to_string(id: QdrantPointId) -> String {
    match id {
        QdrantPointId::Num(value) => value.to_string(),
        QdrantPointId::Uuid(value) => value,
    }
}

/// Convert an rskit string ID into Qdrant's numeric-or-UUID point ID contract.
pub(crate) fn qdrant_point_id_from_string(id: &str) -> AppResult<QdrantPointId> {
    if is_canonical_numeric_id(id) {
        let value = id.parse::<u64>().map_err(|error| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("numeric Qdrant point ID '{id}' is outside u64 bounds"),
            )
            .with_cause(error)
        })?;
        return Ok(QdrantPointId::Num(value));
    }

    let value = uuid::Uuid::parse_str(id).map_err(|error| {
        AppError::new(
            ErrorCode::InvalidInput,
            format!("Qdrant point ID '{id}' must be numeric or a valid UUID"),
        )
        .with_cause(error)
    })?;
    Ok(QdrantPointId::Uuid(value.to_string()))
}

fn is_canonical_numeric_id(id: &str) -> bool {
    !id.is_empty()
        && id.chars().all(|ch| ch.is_ascii_digit())
        && (id == "0" || !id.starts_with('0'))
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

fn unsupported_payload(field: &str) -> AppError {
    AppError::new(
        ErrorCode::InvalidInput,
        format!("unsupported Qdrant payload value for field '{field}'"),
    )
}

#[cfg(test)]
mod tests {
    use rskit_errors::ErrorCode;
    use rskit_vectorstore::SimilarityMetric;

    use super::*;

    #[test]
    fn qdrant_distance_maps_supported_metrics() {
        assert_eq!(qdrant_distance(SimilarityMetric::Cosine).unwrap(), "Cosine");
        assert_eq!(qdrant_distance(SimilarityMetric::Dot).unwrap(), "Dot");
        assert_eq!(qdrant_distance(SimilarityMetric::L2).unwrap(), "Euclid");
    }

    #[test]
    fn qdrant_value_to_payload_rejects_unsupported_returned_values() {
        let err = qdrant_value_to_payload("tags", serde_json::json!([])).unwrap_err();

        assert_eq!(err.code(), ErrorCode::InvalidInput);
        assert!(err.message().contains("tags"));
    }

    #[test]
    fn qdrant_point_id_from_string_accepts_canonical_numeric_strings() {
        assert_eq!(
            qdrant_point_id_from_string("42").unwrap(),
            QdrantPointId::Num(42)
        );
        assert_eq!(
            qdrant_point_id_from_string("0").unwrap(),
            QdrantPointId::Num(0)
        );
    }

    #[test]
    fn qdrant_point_id_from_string_rejects_leading_zero_numeric_strings() {
        let err = qdrant_point_id_from_string("00042").unwrap_err();

        assert_eq!(err.code(), ErrorCode::InvalidInput);
        assert!(err.message().contains("numeric or a valid UUID"));
    }

    #[test]
    fn qdrant_point_id_from_string_accepts_uuid_strings() {
        let id = "550e8400-e29b-41d4-a716-446655440000";

        assert_eq!(
            qdrant_point_id_from_string(id).unwrap(),
            QdrantPointId::Uuid(id.to_owned())
        );
    }

    #[test]
    fn qdrant_point_id_from_string_rejects_non_uuid_strings() {
        let err = qdrant_point_id_from_string("not-a-uuid").unwrap_err();

        assert_eq!(err.code(), ErrorCode::InvalidInput);
        assert!(err.message().contains("numeric or a valid UUID"));
    }
}
