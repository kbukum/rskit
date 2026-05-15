//! JSON Schema generation from Rust types.
//!
//! Thin wrapper around [`schemars`] providing a consistent API for generating
//! JSON Schema documents from any type implementing `JsonSchema`.

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
        .with_cause(err)
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
        .with_cause(err)
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

#[cfg(test)]
mod tests {
    use super::*;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

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
}
