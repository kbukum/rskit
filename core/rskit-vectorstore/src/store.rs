//! Vector store trait definition.

use std::collections::HashMap;

use async_trait::async_trait;
use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::{Deserialize, Serialize};

use crate::VectorStoreLimits;

/// Typed scalar payload value stored alongside vector points.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[non_exhaustive]
#[serde(untagged)]
pub enum PayloadValue {
    /// UTF-8 string value.
    String(String),
    /// Signed integer value.
    Integer(i64),
    /// Floating-point value.
    ///
    /// Values are validated as finite by store/adaptor request validation before
    /// they are accepted for storage or filtering.
    Float(f64),
    /// Boolean value.
    Bool(bool),
}

impl PayloadValue {
    /// Return the value as a string when it is string-typed.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            Self::Integer(_) | Self::Float(_) | Self::Bool(_) => None,
        }
    }

    pub(crate) fn encoded_len(&self) -> usize {
        match self {
            Self::String(value) => value.len(),
            Self::Integer(_) => std::mem::size_of::<i64>(),
            Self::Float(_) => std::mem::size_of::<f64>(),
            Self::Bool(_) => std::mem::size_of::<bool>(),
        }
    }
}

impl From<String> for PayloadValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<&str> for PayloadValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

impl From<i64> for PayloadValue {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}

impl From<i32> for PayloadValue {
    fn from(value: i32) -> Self {
        Self::Integer(i64::from(value))
    }
}

impl From<u32> for PayloadValue {
    fn from(value: u32) -> Self {
        Self::Integer(i64::from(value))
    }
}

impl From<f64> for PayloadValue {
    fn from(value: f64) -> Self {
        Self::Float(value)
    }
}

impl From<f32> for PayloadValue {
    fn from(value: f32) -> Self {
        Self::Float(f64::from(value))
    }
}

impl From<bool> for PayloadValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

/// Payload stored alongside each vector point.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PointPayload {
    /// Metadata fields carried with the point.
    pub fields: HashMap<String, PayloadValue>,
}

impl PointPayload {
    /// Create an empty payload.
    #[must_use]
    pub fn new() -> Self {
        Self {
            fields: HashMap::new(),
        }
    }

    /// Add a payload field.
    #[must_use]
    pub fn with_field(mut self, key: impl Into<String>, value: impl Into<PayloadValue>) -> Self {
        self.fields.insert(key.into(), value.into());
        self
    }

    /// Validate field count and approximate scalar payload bytes against limits.
    pub fn validate_limits(&self, limits: &VectorStoreLimits) -> AppResult<()> {
        if self.fields.len() > limits.max_payload_fields {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!(
                    "vector payload has {} fields, exceeding max_payload_fields {}",
                    self.fields.len(),
                    limits.max_payload_fields
                ),
            ));
        }
        let mut total_bytes = 0usize;
        for (key, payload_value) in &self.fields {
            if let PayloadValue::Float(float_value) = payload_value
                && !float_value.is_finite()
            {
                return Err(AppError::new(
                    ErrorCode::InvalidInput,
                    "vector payload float values must be finite",
                ));
            }
            total_bytes = total_bytes
                .checked_add(key.len())
                .and_then(|bytes| bytes.checked_add(payload_value.encoded_len()))
                .ok_or_else(|| {
                    AppError::new(
                        ErrorCode::InvalidInput,
                        "vector payload byte size overflowed validation bounds",
                    )
                })?;
        }
        if total_bytes > limits.max_payload_bytes {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!(
                    "vector payload is {total_bytes} bytes, exceeding max_payload_bytes {}",
                    limits.max_payload_bytes
                ),
            ));
        }
        Ok(())
    }
}

impl Default for PointPayload {
    fn default() -> Self {
        Self::new()
    }
}

/// A single search result from the vector store.
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// Point identifier.
    pub id: String,
    /// Backend-specific similarity score.
    pub score: f32,
    /// Payload attached to the point.
    pub payload: PointPayload,
}

/// Canonical vector distance/similarity metrics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "lowercase")]
pub enum SimilarityMetric {
    /// Cosine similarity.
    #[default]
    Cosine,
    /// Dot product.
    Dot,
    /// Euclidean L2 distance.
    L2,
}

/// Exact-match metadata filter condition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FilterCondition {
    /// Payload field path.
    pub field: String,
    /// Exact value to match.
    pub equals: PayloadValue,
}

/// Optional filters for search queries.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchFilter {
    /// Filter by exact field match (e.g., platform = "youtube").
    #[serde(default)]
    pub must: Vec<FilterCondition>,
}

impl SearchFilter {
    /// Create an empty filter.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an exact-match condition to the `must` list.
    #[must_use]
    pub fn must_match(mut self, field: impl Into<String>, value: impl Into<PayloadValue>) -> Self {
        self.must.push(FilterCondition {
            field: field.into(),
            equals: value.into(),
        });
        self
    }

    /// Validate filter condition count and approximate scalar bytes against limits.
    pub fn validate_limits(&self, limits: &VectorStoreLimits) -> AppResult<()> {
        if self.must.len() > limits.max_filter_conditions {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!(
                    "vector search filter has {} conditions, exceeding max_filter_conditions {}",
                    self.must.len(),
                    limits.max_filter_conditions
                ),
            ));
        }
        let mut total_bytes = 0usize;
        for condition in &self.must {
            if let PayloadValue::Float(float_value) = &condition.equals
                && !float_value.is_finite()
            {
                return Err(AppError::new(
                    ErrorCode::InvalidInput,
                    "vector filter float values must be finite",
                ));
            }
            total_bytes = total_bytes
                .checked_add(condition.field.len())
                .and_then(|bytes| bytes.checked_add(condition.equals.encoded_len()))
                .ok_or_else(|| {
                    AppError::new(
                        ErrorCode::InvalidInput,
                        "vector filter byte size overflowed validation bounds",
                    )
                })?;
        }
        if total_bytes > limits.max_payload_bytes {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!(
                    "vector search filter is {total_bytes} bytes, exceeding max_payload_bytes {}",
                    limits.max_payload_bytes
                ),
            ));
        }
        Ok(())
    }
}

/// Trait for vector similarity search stores.
#[async_trait]
pub trait VectorStore: Send + Sync {
    /// Ensure a collection exists, creating it if necessary.
    async fn ensure_collection(&self, collection: &str, dimensions: usize) -> AppResult<()>;

    /// Insert or update a vector point.
    async fn upsert(
        &self,
        collection: &str,
        id: &str,
        vector: Vec<f32>,
        payload: PointPayload,
    ) -> AppResult<()>;

    /// Search for similar vectors.
    async fn search(
        &self,
        collection: &str,
        vector: Vec<f32>,
        limit: usize,
        filter: Option<SearchFilter>,
    ) -> AppResult<Vec<SearchResult>>;

    /// Delete a point by ID.
    async fn delete(&self, collection: &str, id: &str) -> AppResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_value_serializes_as_json_scalar() {
        assert_eq!(
            serde_json::to_value(PayloadValue::String("doc".to_owned())).unwrap(),
            serde_json::json!("doc")
        );
        assert_eq!(
            serde_json::to_value(PayloadValue::Integer(42)).unwrap(),
            serde_json::json!(42)
        );
        assert_eq!(
            serde_json::to_value(PayloadValue::Float(1.5)).unwrap(),
            serde_json::json!(1.5)
        );
        assert_eq!(
            serde_json::to_value(PayloadValue::Bool(true)).unwrap(),
            serde_json::json!(true)
        );
    }

    #[test]
    fn payload_value_deserializes_from_json_scalar() {
        assert_eq!(
            serde_json::from_value::<PayloadValue>(serde_json::json!("doc")).unwrap(),
            PayloadValue::String("doc".to_owned())
        );
        assert_eq!(
            serde_json::from_value::<PayloadValue>(serde_json::json!(42)).unwrap(),
            PayloadValue::Integer(42)
        );
        assert_eq!(
            serde_json::from_value::<PayloadValue>(serde_json::json!(1.5)).unwrap(),
            PayloadValue::Float(1.5)
        );
        assert_eq!(
            serde_json::from_value::<PayloadValue>(serde_json::json!(true)).unwrap(),
            PayloadValue::Bool(true)
        );
    }
}
