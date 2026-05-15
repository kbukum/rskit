//! Fluent field-level validator — collects all field errors before returning.
//!
//! # Example
//!
//! ```rust
//! use rskit_validation::Validator;
//!
//! fn validate_user(name: &str, email: &str) -> rskit_errors::AppResult<()> {
//!     Validator::new()
//!         .required("name", name)
//!         .max_length("name", name, 100)
//!         .email("email", email)
//!         .validate()
//! }
//! ```

#![warn(missing_docs)]

use std::fmt::Display;

pub use validator::{self, Validate};

use rskit_errors::{AppError, AppResult, ErrorCode};
use serde_json::Value;

/// Validation helpers for path and text inputs.
pub mod input {
    use rskit_errors::{AppError, AppResult};

    /// Validate that a path-like input cannot traverse outside its base.
    pub fn validate_safe_path(path: &str) -> AppResult<()> {
        reject_unicode_controls("path", path)?;
        if path.is_empty()
            || path.starts_with('/')
            || path.starts_with('\\')
            || path.split(['/', '\\']).any(|segment| {
                segment.is_empty() || segment == "." || segment == ".." || segment.contains(':')
            })
        {
            return Err(AppError::invalid_input(
                "path",
                "path must be relative and must not contain traversal segments",
            ));
        }
        Ok(())
    }

    /// Reject control characters that can hide or reorder text.
    pub fn reject_unicode_controls(field: &str, value: &str) -> AppResult<()> {
        for ch in value.chars() {
            if ch.is_control()
                || matches!(
                    ch,
                    '\u{202A}'..='\u{202E}'
                        | '\u{2066}'..='\u{2069}'
                        | '\u{200E}'
                        | '\u{200F}'
                )
            {
                return Err(AppError::invalid_input(
                    field,
                    "input contains forbidden Unicode control characters",
                ));
            }
        }
        Ok(())
    }
}

// ── FieldError ────────────────────────────────────────────────────────────────

/// A single field validation failure.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FieldError {
    /// The name of the field that failed validation.
    pub field: String,
    /// Human-readable description of the failure.
    pub message: String,
}

// ── Validator ─────────────────────────────────────────────────────────────────

/// Fluent builder that collects field errors and converts to [`AppError`] via
/// [`Validator::validate`].
#[derive(Debug, Default)]
pub struct Validator {
    errors: Vec<FieldError>,
}

impl Validator {
    /// Create a new empty validator.
    pub fn new() -> Self {
        Self::default()
    }

    // ── String checks ─────────────────────────────────────────────────────

    /// Fail if `value` is empty or whitespace-only.
    #[must_use]
    pub fn required(mut self, field: &str, value: &str) -> Self {
        if value.trim().is_empty() {
            self.add(field, "is required");
        }
        self
    }

    /// Fail if `value` has fewer than `min` characters.
    #[must_use]
    pub fn min_length(mut self, field: &str, value: &str, min: usize) -> Self {
        if value.chars().count() < min {
            self.add(field, format!("must be at least {min} characters"));
        }
        self
    }

    /// Fail if `value` exceeds `max` characters.
    #[must_use]
    pub fn max_length(mut self, field: &str, value: &str, max: usize) -> Self {
        if value.chars().count() > max {
            self.add(field, format!("must be at most {max} characters"));
        }
        self
    }

    /// Fail if `value` is not a valid e-mail address.
    #[must_use]
    pub fn email(mut self, field: &str, value: &str) -> Self {
        if !validate_email(value) {
            self.add(field, "must be a valid email address");
        }
        self
    }

    /// Fail if `value` is not a valid HTTP/HTTPS URL.
    #[must_use]
    pub fn url(mut self, field: &str, value: &str) -> Self {
        if !validate_url(value) {
            self.add(field, "must be a valid URL");
        }
        self
    }

    /// Fail if `value` does not match the regular expression `re`.
    #[must_use]
    pub fn pattern(mut self, field: &str, value: &str, re: &str) -> Self {
        match regex::Regex::new(re) {
            Ok(regex) => {
                if !regex.is_match(value) {
                    self.add(field, format!("must match pattern {re}"));
                }
            }
            Err(err) => self.add(field, format!("invalid pattern: {err}")),
        }
        self
    }

    // ── UUID ──────────────────────────────────────────────────────────────

    /// Fail if `value` is not a valid UUID string.
    #[must_use]
    pub fn required_uuid(mut self, field: &str, value: &str) -> Self {
        if !validate_uuid(value) {
            self.add(field, "must be a valid UUID");
        }
        self
    }

    /// Fail if `value` is `Some` but not a valid UUID string.
    #[must_use]
    pub fn optional_uuid(mut self, field: &str, value: Option<&str>) -> Self {
        if let Some(v) = value
            && !validate_uuid(v)
        {
            self.add(field, "must be a valid UUID");
        }
        self
    }

    // ── Numeric ───────────────────────────────────────────────────────────

    /// Fail if `value` is outside the inclusive range `[min, max]`.
    #[must_use]
    pub fn in_range<T: PartialOrd + Display>(
        mut self,
        field: &str,
        value: T,
        min: T,
        max: T,
    ) -> Self {
        if value < min || value > max {
            self.add(field, format!("must be between {min} and {max}"));
        }
        self
    }

    /// Fail if `value` is below `min`.
    #[must_use]
    pub fn min_value<T: PartialOrd + Display>(mut self, field: &str, value: T, min: T) -> Self {
        if value < min {
            self.add(field, format!("must be at least {min}"));
        }
        self
    }

    /// Fail if `value` is above `max`.
    #[must_use]
    pub fn max_value<T: PartialOrd + Display>(mut self, field: &str, value: T, max: T) -> Self {
        if value > max {
            self.add(field, format!("must be {max} or less"));
        }
        self
    }

    // ── Time ──────────────────────────────────────────────────────────────

    /// Fail if `value` (ISO-8601 datetime string) is not before `deadline`.
    #[must_use]
    pub fn before(mut self, field: &str, value: &str, deadline: &str) -> Self {
        match (
            chrono::DateTime::parse_from_rfc3339(value),
            chrono::DateTime::parse_from_rfc3339(deadline),
        ) {
            (Ok(v), Ok(d)) if v >= d => {
                self.add(field, format!("must be before {deadline}"));
            }
            (Err(_), _) => self.add(field, "must be a valid datetime"),
            _ => {}
        }
        self
    }

    /// Fail if `value` (ISO-8601 datetime string) is not after `floor`.
    #[must_use]
    pub fn after(mut self, field: &str, value: &str, floor: &str) -> Self {
        match (
            chrono::DateTime::parse_from_rfc3339(value),
            chrono::DateTime::parse_from_rfc3339(floor),
        ) {
            (Ok(v), Ok(f)) if v <= f => {
                self.add(field, format!("must be after {floor}"));
            }
            (Err(_), _) => self.add(field, "must be a valid datetime"),
            _ => {}
        }
        self
    }

    // ── Enum membership ───────────────────────────────────────────────────

    /// Fail if `value` is not in `allowed`.
    #[must_use]
    pub fn one_of<T: PartialEq + Display>(mut self, field: &str, value: &T, allowed: &[T]) -> Self {
        if !allowed.iter().any(|a| a == value) {
            let list = allowed
                .iter()
                .map(|a| a.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            self.add(field, format!("must be one of: {list}"));
        }
        self
    }

    // ── Composition ───────────────────────────────────────────────────────

    /// Add an error for `field` if `condition` is `false`.
    #[must_use]
    pub fn custom(mut self, condition: bool, field: &str, message: &str) -> Self {
        if !condition {
            self.add(field, message);
        }
        self
    }

    // ── Terminal ──────────────────────────────────────────────────────────

    /// Returns `true` if any validation errors have been accumulated.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Returns a slice of all accumulated field errors.
    pub fn errors(&self) -> &[FieldError] {
        &self.errors
    }

    /// Consume the validator and return `Ok(())` if no errors, or an
    /// [`AppError::invalid_input`] containing all field errors.
    pub fn validate(self) -> AppResult<()> {
        if self.errors.is_empty() {
            return Ok(());
        }
        let detail = self
            .errors
            .iter()
            .map(|e| format!("{}: {}", e.field, e.message))
            .collect::<Vec<_>>()
            .join("; ");
        let fields_json =
            serde_json::to_value(&self.errors).unwrap_or_else(|_| serde_json::Value::Array(vec![]));
        Err(AppError::new(ErrorCode::InvalidInput, detail).with_detail("fields", fields_json))
    }

    // ── Internal ──────────────────────────────────────────────────────────

    fn add(&mut self, field: &str, message: impl Into<String>) {
        self.errors.push(FieldError {
            field: field.to_owned(),
            message: message.into(),
        });
    }
}

// ── JSON Schema Validation ────────────────────────────────────────────────────

/// A single validation error with a JSON-pointer path.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ValidationError {
    /// JSON Pointer path to the failing value.
    pub path: String,
    /// Human-readable validation failure message.
    pub message: String,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.path.is_empty() {
            write!(f, "{}", self.message)
        } else {
            write!(f, "{}: {}", self.path, self.message)
        }
    }
}

/// Outcome of validating a value against a schema.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ValidationResult {
    /// Whether the value satisfied the schema.
    pub valid: bool,
    /// Validation failures. Empty when `valid` is true.
    pub errors: Vec<ValidationError>,
}

/// Reusable compiled JSON Schema validator.
pub struct CompiledSchema {
    validator: jsonschema::Validator,
}

impl CompiledSchema {
    /// Validate a JSON value against this compiled schema.
    pub fn validate(&self, value: &Value) -> ValidationResult {
        validation_result(&self.validator, value)
    }
}

/// Compile a JSON Schema once for repeated validation.
pub fn compile(schema: &Value) -> AppResult<CompiledSchema> {
    jsonschema::validator_for(schema)
        .map(|validator| CompiledSchema { validator })
        .map_err(|err| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("invalid JSON Schema: {err}"),
            )
            .with_cause(err)
        })
}

/// Validate a JSON value against a JSON Schema.
pub fn validate(schema: &Value, value: &Value) -> ValidationResult {
    let validator = match compile(schema) {
        Ok(validator) => validator,
        Err(err) => {
            return ValidationResult {
                valid: false,
                errors: vec![ValidationError {
                    path: String::new(),
                    message: err.message().to_owned(),
                }],
            };
        }
    };

    validator.validate(value)
}

fn validation_result(validator: &jsonschema::Validator, value: &Value) -> ValidationResult {
    let errors = validator
        .iter_errors(value)
        .map(|err| ValidationError {
            path: err.instance_path().to_string(),
            message: err.to_string(),
        })
        .collect::<Vec<_>>();

    ValidationResult {
        valid: errors.is_empty(),
        errors,
    }
}

/// Validate structured model output against a JSON Schema 2020-12-compatible schema subset.
pub fn validate_structured_output(schema: &Value, value: &Value) -> ValidationResult {
    validate(schema, value)
}

// ── Free functions ────────────────────────────────────────────────────────────

/// Returns `true` if `value` looks like a valid e-mail address.
pub fn validate_email(value: &str) -> bool {
    let parts: Vec<&str> = value.splitn(2, '@').collect();
    if parts.len() != 2 {
        return false;
    }
    let local = parts[0];
    let domain = parts[1];
    !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
}

/// Returns `true` if `value` is an absolute HTTP or HTTPS URL.
pub fn validate_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}

/// Returns `true` if `value` is a valid UUID (any version, hyphenated or not).
pub fn validate_uuid(value: &str) -> bool {
    value.parse::<uuid::Uuid>().is_ok()
}

// ── chrono re-export guard ────────────────────────────────────────────────────
// We use chrono internally but do not re-export it to avoid version conflicts.

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn required_fails_on_empty() {
        let r = Validator::new().required("name", "").validate();
        assert!(r.is_err());
    }

    #[test]
    fn required_passes_non_empty() {
        let r = Validator::new().required("name", "Alice").validate();
        assert!(r.is_ok());
    }

    #[test]
    fn email_fails_on_invalid() {
        let r = Validator::new().email("email", "notanemail").validate();
        assert!(r.is_err());
    }

    #[test]
    fn email_passes_on_valid() {
        let r = Validator::new()
            .email("email", "user@example.com")
            .validate();
        assert!(r.is_ok());
    }

    #[test]
    fn multiple_errors_collected() {
        let v =
            Validator::new()
                .required("name", "")
                .max_length("bio", "x".repeat(201).as_str(), 200);
        assert_eq!(v.errors().len(), 2);
    }

    #[test]
    fn in_range_passes_at_boundaries() {
        let r = Validator::new().in_range("age", 0u32, 0, 120).validate();
        assert!(r.is_ok());
        let r = Validator::new().in_range("age", 120u32, 0, 120).validate();
        assert!(r.is_ok());
    }

    #[test]
    fn in_range_fails_outside() {
        let r = Validator::new().in_range("age", 121u32, 0, 120).validate();
        assert!(r.is_err());
    }

    #[test]
    fn uuid_validates() {
        assert!(validate_uuid("550e8400-e29b-41d4-a716-446655440000"));
        assert!(!validate_uuid("not-a-uuid"));
    }

    #[test]
    fn path_validation_rejects_traversal_and_mixed_separators() {
        assert!(input::validate_safe_path("tenant/report.json").is_ok());
        assert!(input::validate_safe_path("../secret").is_err());
        assert!(input::validate_safe_path("tenant\\..\\secret").is_err());
        assert!(input::validate_safe_path("tenant/..\\secret").is_err());
    }

    #[test]
    fn unicode_validation_rejects_control_characters() {
        assert!(input::reject_unicode_controls("identifier", "safe-id").is_ok());
        assert!(input::reject_unicode_controls("identifier", "safe\u{202e}txt").is_err());
        assert!(input::reject_unicode_controls("identifier", "павел").is_ok());
    }

    // ── JSON Schema validate tests ──────────────────────────────────────

    #[test]
    fn validate_correct_object() {
        let schema = json!({
            "type": "object",
            "required": ["name", "age"],
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "integer"}
            }
        });
        let value = json!({"name": "Alice", "age": 30});
        let result = validate(&schema, &value);
        assert!(result.valid, "errors: {:?}", result.errors);
    }

    #[test]
    fn validate_missing_required_field() {
        let schema = json!({
            "type": "object",
            "required": ["name", "age"],
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "integer"}
            }
        });
        let value = json!({"name": "Alice"});
        let result = validate(&schema, &value);
        assert!(!result.valid);
        assert_eq!(result.errors.len(), 1);
        assert!(result.errors[0].message.contains("age"));
    }

    #[test]
    fn validate_wrong_type() {
        let schema = json!({"type": "string"});
        let value = json!(42);
        let result = validate(&schema, &value);
        assert!(!result.valid);
        assert!(result.errors[0].message.contains("string"));
    }

    #[test]
    fn validate_string_min_length() {
        let schema = json!({"type": "string", "minLength": 3});
        assert!(validate(&schema, &json!("abc")).valid);
        assert!(!validate(&schema, &json!("ab")).valid);
    }

    #[test]
    fn validate_string_max_length() {
        let schema = json!({"type": "string", "maxLength": 5});
        assert!(validate(&schema, &json!("hello")).valid);
        assert!(!validate(&schema, &json!("toolong")).valid);
    }

    #[test]
    fn validate_number_minimum() {
        let schema = json!({"type": "number", "minimum": 0});
        assert!(validate(&schema, &json!(5)).valid);
        assert!(!validate(&schema, &json!(-1)).valid);
    }

    #[test]
    fn validate_number_maximum() {
        let schema = json!({"type": "number", "maximum": 100});
        assert!(validate(&schema, &json!(50)).valid);
        assert!(!validate(&schema, &json!(101)).valid);
    }

    #[test]
    fn validate_array_items() {
        let schema = json!({
            "type": "array",
            "items": {"type": "string"}
        });
        assert!(validate(&schema, &json!(["a", "b"])).valid);
        let r = validate(&schema, &json!(["a", 1]));
        assert!(!r.valid);
        assert_eq!(r.errors.len(), 1);
    }

    #[test]
    fn validate_array_min_items() {
        let schema = json!({"type": "array", "minItems": 2});
        assert!(validate(&schema, &json!([1, 2])).valid);
        assert!(!validate(&schema, &json!([1])).valid);
    }

    #[test]
    fn validate_array_max_items() {
        let schema = json!({"type": "array", "maxItems": 2});
        assert!(validate(&schema, &json!([1])).valid);
        assert!(!validate(&schema, &json!([1, 2, 3])).valid);
    }

    #[test]
    fn validate_enum() {
        let schema = json!({"enum": ["red", "green", "blue"]});
        assert!(validate(&schema, &json!("red")).valid);
        assert!(!validate(&schema, &json!("yellow")).valid);
    }

    #[test]
    fn validate_nested_object() {
        let schema = json!({
            "type": "object",
            "properties": {
                "user": {
                    "type": "object",
                    "required": ["name"],
                    "properties": {
                        "name": {"type": "string"}
                    }
                }
            }
        });
        let r = validate(&schema, &json!({"user": {}}));
        assert!(!r.valid);
        assert!(r.errors[0].message.contains("name"));
    }

    #[test]
    fn validate_boolean_type() {
        let schema = json!({"type": "boolean"});
        assert!(validate(&schema, &json!(true)).valid);
        assert!(!validate(&schema, &json!("true")).valid);
    }

    #[test]
    fn validate_null_type() {
        let schema = json!({"type": "null"});
        assert!(validate(&schema, &json!(null)).valid);
        assert!(!validate(&schema, &json!(0)).valid);
    }

    #[test]
    fn validate_integer_rejects_float() {
        let schema = json!({"type": "integer"});
        assert!(validate(&schema, &json!(42)).valid);
        assert!(!validate(&schema, &json!(3.25)).valid);
    }

    #[test]
    fn validate_error_display_with_path() {
        let e = ValidationError {
            path: "user.name".to_string(),
            message: "required".to_string(),
        };
        assert_eq!(format!("{e}"), "user.name: required");
    }

    #[test]
    fn validate_error_display_empty_path() {
        let e = ValidationError {
            path: String::new(),
            message: "type mismatch".to_string(),
        };
        assert_eq!(format!("{e}"), "type mismatch");
    }

    #[test]
    fn validate_empty_schema_accepts_anything() {
        let schema = json!({});
        assert!(validate(&schema, &json!(42)).valid);
        assert!(validate(&schema, &json!("hi")).valid);
        assert!(validate(&schema, &json!(null)).valid);
    }

    #[test]
    fn validate_invalid_schema_returns_error_result() {
        let schema = json!({"type": "not-a-json-schema-type"});
        let result = validate(&schema, &json!("value"));
        assert!(!result.valid);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].path, "");
        assert!(result.errors[0].message.contains("invalid JSON Schema"));
    }

    #[test]
    fn compiled_schema_reuses_validator() {
        let schema = json!({"type": "string"});
        let validator = compile(&schema).unwrap();

        assert!(validator.validate(&json!("value")).valid);
        assert!(!validator.validate(&json!(42)).valid);
    }

    #[test]
    fn validate_multiple_missing_required() {
        let schema = json!({
            "type": "object",
            "required": ["a", "b", "c"]
        });
        let r = validate(&schema, &json!({}));
        assert!(!r.valid);
        assert_eq!(r.errors.len(), 3);
    }
}
