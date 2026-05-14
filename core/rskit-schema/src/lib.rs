//! JSON Schema generation and validation from Rust types.
//!
//! Thin wrapper around [`schemars`] providing a consistent API for generating
//! JSON Schema documents from any type implementing `JsonSchema`, plus a
//! runtime validator for checking JSON values against schemas.

#![warn(missing_docs)]

use rskit_errors::{AppError, AppResult, ErrorCode};
pub use schemars::JsonSchema;
use schemars::SchemaGenerator;
use serde_json::Value;

/// Standard JSON Schema type alias.
pub type Json = serde_json::Value;

// ── Schema Generation ───────────────────────────────────────────────────────

/// Generate a JSON Schema from a type implementing `JsonSchema`.
pub fn generate<T: JsonSchema>() -> AppResult<Json> {
    let schema = SchemaGenerator::default().into_root_schema_for::<T>();
    serde_json::to_value(schema).map_err(|err| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to serialize generated JSON schema: {err}"),
        )
    })
}

/// Options for customizing schema generation.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Override the schema title.
    pub title: Option<String>,
    /// Override the schema description.
    pub description: Option<String>,
}

/// Generate a JSON Schema with custom options.
pub fn generate_with<T: JsonSchema>(opts: Options) -> AppResult<Json> {
    let schema = SchemaGenerator::default().into_root_schema_for::<T>();
    let mut value = serde_json::to_value(schema).map_err(|err| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to serialize generated JSON schema: {err}"),
        )
    })?;

    if let Some(obj) = value.as_object_mut() {
        if let Some(title) = opts.title {
            obj.insert("title".to_string(), Value::String(title));
        }
        if let Some(desc) = opts.description {
            obj.insert("description".to_string(), Value::String(desc));
        }
    }

    Ok(value)
}

// ── Schema Validation ───────────────────────────────────────────────────────

/// A single validation error with a JSON-pointer path.
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether the value satisfied the schema.
    pub valid: bool,
    /// Validation failures. Empty when `valid` is true.
    pub errors: Vec<ValidationError>,
}

/// Validate a JSON value against a JSON Schema.
pub fn validate(schema: &Value, value: &Value) -> ValidationResult {
    let validator = match jsonschema::validator_for(schema) {
        Ok(validator) => validator,
        Err(err) => {
            return ValidationResult {
                valid: false,
                errors: vec![ValidationError {
                    path: String::new(),
                    message: format!("invalid JSON Schema: {err}"),
                }],
            };
        }
    };

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

#[cfg(test)]
mod tests {
    use super::*;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    use serde_json::json;

    #[derive(Serialize, Deserialize, JsonSchema)]
    struct Simple {
        name: String,
        age: u32,
    }

    #[derive(Serialize, Deserialize, JsonSchema)]
    #[allow(dead_code)]
    struct WithDefaults {
        query: String,
        #[serde(default)]
        max_results: u32,
    }

    #[derive(Serialize, Deserialize, JsonSchema)]
    struct Nested {
        user: Simple,
        tags: Vec<String>,
    }

    // ── generate tests ──────────────────────────────────────────────────

    #[test]
    fn test_generate_simple() {
        let schema = generate::<Simple>().unwrap();
        assert!(schema.is_object());
        let obj = schema.as_object().unwrap();
        assert_eq!(obj.get("type").and_then(|v| v.as_str()), Some("object"));

        let props = obj.get("properties").unwrap().as_object().unwrap();
        assert!(props.contains_key("name"));
        assert!(props.contains_key("age"));
    }

    #[test]
    fn test_generate_required_fields() {
        let schema = generate::<Simple>().unwrap();
        let obj = schema.as_object().unwrap();
        let required = obj.get("required").and_then(|v| v.as_array()).unwrap();
        let names: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();
        assert!(names.contains(&"name"));
        assert!(names.contains(&"age"));
    }

    #[test]
    fn test_generate_nested() {
        let schema = generate::<Nested>().unwrap();
        let obj = schema.as_object().unwrap();
        let props = obj.get("properties").unwrap().as_object().unwrap();
        assert!(props.contains_key("user"));
        assert!(props.contains_key("tags"));
    }

    #[test]
    fn test_generate_with_title() {
        let schema = generate_with::<Simple>(Options {
            title: Some("Person".to_string()),
            ..Default::default()
        })
        .unwrap();
        let obj = schema.as_object().unwrap();
        assert_eq!(obj.get("title").and_then(|v| v.as_str()), Some("Person"));
    }

    #[test]
    fn test_generate_with_description() {
        let schema = generate_with::<Simple>(Options {
            description: Some("A person record".to_string()),
            ..Default::default()
        })
        .unwrap();
        let obj = schema.as_object().unwrap();
        assert_eq!(
            obj.get("description").and_then(|v| v.as_str()),
            Some("A person record")
        );
    }

    #[test]
    fn test_string_type_properties() {
        let schema = generate::<Simple>().unwrap();
        let props = schema
            .as_object()
            .unwrap()
            .get("properties")
            .unwrap()
            .as_object()
            .unwrap();
        let name_type = props
            .get("name")
            .unwrap()
            .as_object()
            .unwrap()
            .get("type")
            .and_then(|v| v.as_str());
        assert_eq!(name_type, Some("string"));
    }

    #[test]
    fn test_integer_type_properties() {
        let schema = generate::<Simple>().unwrap();
        let props = schema
            .as_object()
            .unwrap()
            .get("properties")
            .unwrap()
            .as_object()
            .unwrap();
        let age_type = props
            .get("age")
            .unwrap()
            .as_object()
            .unwrap()
            .get("type")
            .and_then(|v| v.as_str());
        assert_eq!(age_type, Some("integer"));
    }

    // ── validate tests ──────────────────────────────────────────────────

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
