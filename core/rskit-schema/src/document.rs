//! Typed JSON Schema document wrapper.

use rskit_errors::AppResult;
use serde_json::Value;

use crate::{Json, ValidationLimits, limits::check_json_limits};

/// Owned JSON Schema document that has passed structural limit checks.
#[derive(Debug, Clone, PartialEq)]
pub struct SchemaDocument {
    value: Json,
}

impl SchemaDocument {
    /// Create a schema document with default structural limits.
    pub fn new(value: Json) -> AppResult<Self> {
        Self::with_limits(value, ValidationLimits::default())
    }

    /// Create a schema document with custom structural limits.
    pub fn with_limits(value: Json, limits: ValidationLimits) -> AppResult<Self> {
        check_json_limits("schema", &value, limits)?;
        Ok(Self { value })
    }

    /// Borrow the raw JSON schema value.
    pub fn as_json(&self) -> &Value {
        &self.value
    }

    /// Consume this document and return the raw JSON schema value.
    #[must_use]
    pub fn into_json(self) -> Json {
        self.value
    }
}
