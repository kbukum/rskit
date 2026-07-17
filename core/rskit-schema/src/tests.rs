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
fn generate_document_helpers_return_schema_documents() {
    let document = generate_document::<Simple>().unwrap();
    assert!(document.as_json().is_object());

    let document = generate_with_options::<Simple>(Options {
        title: Some("Override".to_string()),
        description: Some("Generated document".to_string()),
    })
    .unwrap();
    let obj = document.as_json().as_object().unwrap();
    assert_eq!(obj.get("title").and_then(|v| v.as_str()), Some("Override"));
    assert_eq!(
        obj.get("description").and_then(|v| v.as_str()),
        Some("Generated document")
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
fn validation_result_valid_constructor_is_empty_success() {
    let result = ValidationResult::valid();
    assert!(result.valid);
    assert!(result.errors.is_empty());
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
fn compile_with_default_limits_accepts_generated_schema() {
    let schema = generate::<Nested>().unwrap();
    let validator = compile(&schema).unwrap();

    let result = validator.validate(&json!({
        "user": {"name": "Alice", "age": 30},
        "tags": ["admin"]
    }));
    assert!(result.valid, "errors: {:?}", result.errors);
}

#[test]
fn validate_with_options_rejects_excessive_depth() {
    let schema = json!({"type": "array"});
    let value = json!([[[["too deep"]]]]);
    let options = ValidationOptions {
        limits: ValidationLimits::new(3, 100),
    };

    let err = validate_with_options(&schema, &value, options).unwrap_err();
    assert_eq!(err.code(), rskit_errors::ErrorCode::InvalidInput);
}

#[test]
fn validate_with_options_rejects_excessive_string_bytes() {
    let schema = json!({"type": "string"});
    let value = json!("abcdef");
    let options = ValidationOptions {
        limits: ValidationLimits::new(10, 100).with_max_string_bytes(5),
    };

    let err = validate_with_options(&schema, &value, options).unwrap_err();
    assert_eq!(err.code(), rskit_errors::ErrorCode::InvalidInput);
}

#[test]
fn validate_with_options_rejects_excessive_node_count() {
    let schema = json!({});
    let value = json!({"a": [1, 2, 3]});
    let options = ValidationOptions {
        limits: ValidationLimits::new(10, 3),
    };

    let err = validate_with_options(&schema, &value, options).unwrap_err();
    assert_eq!(err.code(), rskit_errors::ErrorCode::InvalidInput);
}

#[test]
fn compile_with_options_rejects_excessive_key_bytes() {
    let schema = json!({
        "type": "object",
        "properties": {
            "too_long": {"type": "string"}
        }
    });
    let options = ValidationOptions {
        limits: ValidationLimits::new(10, 100).with_max_key_bytes(4),
    };

    let err = match compile_with_options(&schema, options) {
        Ok(_) => panic!("schema with excessive key bytes should fail"),
        Err(err) => err,
    };
    assert_eq!(err.code(), rskit_errors::ErrorCode::InvalidInput);
}

#[test]
fn compile_with_options_rejects_total_string_bytes() {
    let schema = json!({
        "type": "object",
        "description": "abcdef",
        "title": "ghijkl"
    });
    let options = ValidationOptions {
        limits: ValidationLimits::new(10, 100).with_max_total_string_bytes(20),
    };

    let err = match compile_with_options(&schema, options) {
        Ok(_) => panic!("schema with excessive total string bytes should fail"),
        Err(err) => err,
    };
    assert_eq!(err.code(), rskit_errors::ErrorCode::InvalidInput);
}

#[test]
fn schema_document_preserves_json_after_limit_check() {
    let schema = generate::<Simple>().unwrap();
    let document = SchemaDocument::new(schema.clone()).unwrap();

    assert_eq!(document.as_json(), &schema);
    assert_eq!(document.into_json(), schema);
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
