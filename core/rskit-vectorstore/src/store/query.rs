//! Search request/response model: metrics, filters, queries, and results.

use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::{Deserialize, Serialize};

use super::point::{PayloadValue, PointPayload};
use crate::VectorStoreLimits;

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

/// A similarity search request against a collection.
#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    /// Query vector to score candidates against.
    pub vector: Vec<f32>,
    /// Maximum number of results to return.
    pub limit: usize,
    /// Optional metadata filter applied before scoring.
    pub filter: Option<SearchFilter>,
}

impl SearchQuery {
    /// Create a search query for `vector` returning up to `limit` results.
    #[must_use]
    pub fn new(vector: Vec<f32>, limit: usize) -> Self {
        Self {
            vector,
            limit,
            filter: None,
        }
    }

    /// Attach a metadata filter to the query.
    #[must_use]
    pub fn with_filter(mut self, filter: SearchFilter) -> Self {
        self.filter = Some(filter);
        self
    }
}
