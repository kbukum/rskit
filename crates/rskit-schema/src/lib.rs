//! JSON Schema generation and validation from Rust types.
//!
//! Thin wrapper around [`schemars`] providing a consistent API for generating
//! JSON Schema documents from any type implementing `JsonSchema`, plus a
//! runtime validator for checking JSON values against schemas.

pub use schemars::JsonSchema;
use schemars::SchemaGenerator;
use serde_json::Value;

/// Standard JSON Schema type alias.
pub type Json = serde_json::Value;

// ── Schema Generation ───────────────────────────────────────────────────────

/// Generate a JSON Schema from a type implementing `JsonSchema`.
pub fn generate<T: JsonSchema>() -> Json {
    let schema = SchemaGenerator::default().into_root_schema_for::<T>();
    serde_json::to_value(schema).unwrap_or_default()
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
pub fn generate_with<T: JsonSchema>(opts: Options) -> Json {
    let schema = SchemaGenerator::default().into_root_schema_for::<T>();
    let mut value = serde_json::to_value(schema).unwrap_or_default();

    if let Some(obj) = value.as_object_mut() {
        if let Some(title) = opts.title {
            obj.insert("title".to_string(), Value::String(title));
        }
        if let Some(desc) = opts.description {
            obj.insert("description".to_string(), Value::String(desc));
        }
    }

    value
}

// ── Schema Validation ───────────────────────────────────────────────────────

/// A single validation error with a JSON-pointer path.
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub path: String,
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
    pub valid: bool,
    pub errors: Vec<ValidationError>,
}

/// Validate a JSON value against a JSON Schema.
pub fn validate(schema: &Value, value: &Value) -> ValidationResult {
    let mut errors = Vec::new();
    validate_value(schema, value, "", &mut errors);
    ValidationResult {
        valid: errors.is_empty(),
        errors,
    }
}

fn validate_value(schema: &Value, value: &Value, path: &str, errors: &mut Vec<ValidationError>) {
    let Some(obj) = schema.as_object() else {
        return;
    };

    // type check
    if let Some(type_val) = obj.get("type") {
        if let Some(expected) = type_val.as_str() {
            if !type_matches(expected, value) {
                errors.push(ValidationError {
                    path: path.to_string(),
                    message: format!(
                        "expected type \"{expected}\", got {}",
                        json_type_name(value)
                    ),
                });
                return; // no point checking further constraints
            }
        }
    }

    // enum
    if let Some(enum_vals) = obj.get("enum").and_then(|v| v.as_array()) {
        if !enum_vals.iter().any(|e| e == value) {
            errors.push(ValidationError {
                path: path.to_string(),
                message: format!("value not in enum: {value}"),
            });
        }
    }

    // string constraints
    if let Some(s) = value.as_str() {
        if let Some(min) = obj.get("minLength").and_then(|v| v.as_u64()) {
            if (s.len() as u64) < min {
                errors.push(ValidationError {
                    path: path.to_string(),
                    message: format!("string length {} < minLength {min}", s.len()),
                });
            }
        }
        if let Some(max) = obj.get("maxLength").and_then(|v| v.as_u64()) {
            if (s.len() as u64) > max {
                errors.push(ValidationError {
                    path: path.to_string(),
                    message: format!("string length {} > maxLength {max}", s.len()),
                });
            }
        }
    }

    // number constraints
    if value.is_number() {
        if let Some(n) = value.as_f64() {
            if let Some(min) = obj.get("minimum").and_then(|v| v.as_f64()) {
                if n < min {
                    errors.push(ValidationError {
                        path: path.to_string(),
                        message: format!("value {n} < minimum {min}"),
                    });
                }
            }
            if let Some(max) = obj.get("maximum").and_then(|v| v.as_f64()) {
                if n > max {
                    errors.push(ValidationError {
                        path: path.to_string(),
                        message: format!("value {n} > maximum {max}"),
                    });
                }
            }
        }
    }

    // object constraints
    if let Some(map) = value.as_object() {
        // required fields
        if let Some(required) = obj.get("required").and_then(|v| v.as_array()) {
            for req in required {
                if let Some(field) = req.as_str() {
                    if !map.contains_key(field) {
                        errors.push(ValidationError {
                            path: child_path(path, field),
                            message: format!("required field \"{field}\" is missing"),
                        });
                    }
                }
            }
        }

        // properties
        if let Some(props) = obj.get("properties").and_then(|v| v.as_object()) {
            for (key, prop_schema) in props {
                if let Some(prop_value) = map.get(key) {
                    validate_value(prop_schema, prop_value, &child_path(path, key), errors);
                }
            }
        }
    }

    // array constraints
    if let Some(arr) = value.as_array() {
        if let Some(min) = obj.get("minItems").and_then(|v| v.as_u64()) {
            if (arr.len() as u64) < min {
                errors.push(ValidationError {
                    path: path.to_string(),
                    message: format!("array length {} < minItems {min}", arr.len()),
                });
            }
        }
        if let Some(max) = obj.get("maxItems").and_then(|v| v.as_u64()) {
            if (arr.len() as u64) > max {
                errors.push(ValidationError {
                    path: path.to_string(),
                    message: format!("array length {} > maxItems {max}", arr.len()),
                });
            }
        }

        // items schema
        if let Some(item_schema) = obj.get("items") {
            for (i, item) in arr.iter().enumerate() {
                validate_value(item_schema, item, &format!("{path}[{i}]"), errors);
            }
        }
    }
}

fn type_matches(expected: &str, value: &Value) -> bool {
    match expected {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "number" => value.is_number(),
        "integer" => value.is_i64() || value.is_u64(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => true,
    }
}

fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                "integer"
            } else {
                "number"
            }
        }
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn child_path(parent: &str, child: &str) -> String {
    if parent.is_empty() {
        child.to_string()
    } else {
        format!("{parent}.{child}")
    }
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
        let schema = generate::<Simple>();
        assert!(schema.is_object());
        let obj = schema.as_object().unwrap();
        assert_eq!(obj.get("type").and_then(|v| v.as_str()), Some("object"));

        let props = obj.get("properties").unwrap().as_object().unwrap();
        assert!(props.contains_key("name"));
        assert!(props.contains_key("age"));
    }

    #[test]
    fn test_generate_required_fields() {
        let schema = generate::<Simple>();
        let obj = schema.as_object().unwrap();
        let required = obj.get("required").and_then(|v| v.as_array()).unwrap();
        let names: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();
        assert!(names.contains(&"name"));
        assert!(names.contains(&"age"));
    }

    #[test]
    fn test_generate_nested() {
        let schema = generate::<Nested>();
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
        });
        let obj = schema.as_object().unwrap();
        assert_eq!(obj.get("title").and_then(|v| v.as_str()), Some("Person"));
    }

    #[test]
    fn test_generate_with_description() {
        let schema = generate_with::<Simple>(Options {
            description: Some("A person record".to_string()),
            ..Default::default()
        });
        let obj = schema.as_object().unwrap();
        assert_eq!(
            obj.get("description").and_then(|v| v.as_str()),
            Some("A person record")
        );
    }

    #[test]
    fn test_string_type_properties() {
        let schema = generate::<Simple>();
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
        let schema = generate::<Simple>();
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
        assert!(r.errors[0].path.contains("name"));
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
        assert!(!validate(&schema, &json!(3.14)).valid);
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
