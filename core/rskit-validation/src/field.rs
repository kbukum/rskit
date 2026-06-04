//! Field-level validation error type.

/// A single field validation failure.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FieldError {
    /// The name of the field that failed validation.
    pub field: String,
    /// Human-readable description of the failure.
    pub message: String,
}
