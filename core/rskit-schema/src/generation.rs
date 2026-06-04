//! JSON Schema generation from Rust types.

use rskit_errors::{AppError, AppResult, ErrorCode};
use schemars::{JsonSchema, SchemaGenerator};
use serde_json::Value;

use crate::{Json, SchemaDocument};

/// Options for customizing schema generation.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Override the schema title.
    pub title: Option<String>,
    /// Override the schema description.
    pub description: Option<String>,
}

/// Generate a JSON Schema from a type implementing [`JsonSchema`].
pub fn generate<T: JsonSchema>() -> AppResult<Json> {
    generate_with::<T>(Options::default())
}

/// Generate a typed JSON Schema document from a type implementing [`JsonSchema`].
pub fn generate_document<T: JsonSchema>() -> AppResult<SchemaDocument> {
    SchemaDocument::new(generate::<T>()?)
}

/// Generate a typed JSON Schema document with custom options.
pub fn generate_with_options<T: JsonSchema>(opts: Options) -> AppResult<SchemaDocument> {
    SchemaDocument::new(generate_with::<T>(opts)?)
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
