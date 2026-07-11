//! Typed tool IO wrappers.

use std::collections::HashMap;
use std::ops::Index;

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_schema::Json;
use serde::{Deserialize, Deserializer, Serialize, de};

/// Machine-validatable JSON Schema for tool input or output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ToolSchema(Json);

impl ToolSchema {
    /// Create a schema from a JSON object value.
    pub fn new(value: Json) -> AppResult<Self> {
        if value.is_object() {
            Ok(Self(value))
        } else {
            Err(AppError::new(
                ErrorCode::InvalidInput,
                "tool schema must be a JSON object",
            ))
        }
    }

    /// Create the permissive object schema used for tools without typed params.
    #[must_use]
    pub fn any_object() -> Self {
        Self(serde_json::json!({"type": "object"}))
    }

    /// Borrow the underlying JSON Schema document.
    #[must_use]
    pub const fn as_json(&self) -> &Json {
        &self.0
    }

    /// Borrow this schema as a JSON object.
    #[must_use]
    pub fn as_object(&self) -> Option<&serde_json::Map<String, Json>> {
        self.0.as_object()
    }

    /// Return whether this schema is represented by a JSON object.
    #[must_use]
    pub fn is_object(&self) -> bool {
        self.0.is_object()
    }

    /// Consume this wrapper and return the JSON Schema document.
    #[must_use]
    pub fn into_json(self) -> Json {
        self.0
    }
}

impl Default for ToolSchema {
    fn default() -> Self {
        Self::any_object()
    }
}

impl TryFrom<Json> for ToolSchema {
    type Error = AppError;

    fn try_from(value: Json) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl<'de> Deserialize<'de> for ToolSchema {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(Json::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

/// Validated tool invocation input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ToolInput(Json);

impl ToolInput {
    /// Create tool input from a JSON object value.
    pub fn new(value: Json) -> AppResult<Self> {
        if value.is_object() {
            Ok(Self(value))
        } else {
            Err(AppError::new(
                ErrorCode::InvalidInput,
                "tool input must be a JSON object",
            ))
        }
    }

    /// Create empty object input.
    #[must_use]
    pub fn empty() -> Self {
        Self(Json::Object(serde_json::Map::new()))
    }

    /// Create tool input from a JSON object map.
    #[must_use]
    pub const fn from_object(object: serde_json::Map<String, Json>) -> Self {
        Self(Json::Object(object))
    }

    /// Borrow the JSON object payload.
    #[must_use]
    pub const fn as_json(&self) -> &Json {
        &self.0
    }

    /// Consume this wrapper and return the JSON object payload.
    #[must_use]
    pub fn into_json(self) -> Json {
        self.0
    }
}

impl Default for ToolInput {
    fn default() -> Self {
        Self::empty()
    }
}

impl TryFrom<Json> for ToolInput {
    type Error = AppError;

    fn try_from(value: Json) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl<'de> Deserialize<'de> for ToolInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(Json::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

/// Structured tool output or metadata value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ToolOutput(Json);

impl ToolOutput {
    /// Create structured output from a serializable value.
    pub fn from_serializable<T: Serialize>(value: &T) -> AppResult<Self> {
        serde_json::to_value(value).map(Self).map_err(|err| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to serialize tool output: {err}"),
            )
            .with_cause(err)
        })
    }

    /// Borrow the structured JSON output.
    #[must_use]
    pub const fn as_json(&self) -> &Json {
        &self.0
    }

    /// Consume this wrapper and return the structured JSON output.
    #[must_use]
    pub fn into_json(self) -> Json {
        self.0
    }
}

impl From<Json> for ToolOutput {
    fn from(value: Json) -> Self {
        Self(value)
    }
}

impl PartialEq<Json> for ToolOutput {
    fn eq(&self, other: &Json) -> bool {
        &self.0 == other
    }
}

impl Index<&str> for ToolOutput {
    type Output = Json;

    fn index(&self, index: &str) -> &Self::Output {
        &self.0[index]
    }
}

/// Metadata attached to a tool context or result.
pub type ToolMetadata = HashMap<String, ToolOutput>;

#[cfg(test)]
mod tests {
    use super::*;
    use serde::ser::{Error as _, SerializeStruct};

    #[test]
    fn schema_deserialization_rejects_non_object() {
        let err = serde_json::from_str::<ToolSchema>(r#""not-a-schema""#).unwrap_err();
        assert!(
            err.to_string()
                .contains("tool schema must be a JSON object")
        );
    }

    #[test]
    fn input_deserialization_rejects_non_object() {
        let err = serde_json::from_str::<ToolInput>("[]").unwrap_err();
        assert!(err.to_string().contains("tool input must be a JSON object"));
    }

    #[test]
    fn output_serialization_error_has_context() {
        struct FailingOutput;

        impl Serialize for FailingOutput {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                let mut state = serializer.serialize_struct("FailingOutput", 1)?;
                state.serialize_field("bad", &"ok")?;
                state.end()?;
                Err(S::Error::custom("boom"))
            }
        }

        let err = ToolOutput::from_serializable(&FailingOutput).unwrap_err();
        assert!(
            err.message()
                .contains("failed to serialize tool output: boom")
        );
    }

    #[test]
    fn schema_and_input_helpers_preserve_json_object_contract() {
        let schema = ToolSchema::any_object();
        assert!(schema.is_object());
        assert_eq!(schema.as_json()["type"], "object");
        assert_eq!(
            ToolSchema::default().into_json(),
            serde_json::json!({"type": "object"})
        );
        assert_eq!(
            ToolSchema::try_from(serde_json::json!("string"))
                .unwrap_err()
                .code(),
            ErrorCode::InvalidInput
        );

        let mut object = serde_json::Map::new();
        object.insert("query".to_owned(), serde_json::json!("rust"));
        let input = ToolInput::from_object(object);
        assert_eq!(input.as_json()["query"], "rust");
        assert_eq!(ToolInput::empty().into_json(), serde_json::json!({}));
        assert_eq!(
            ToolInput::try_from(serde_json::json!(["not", "object"]))
                .unwrap_err()
                .code(),
            ErrorCode::InvalidInput
        );
    }

    #[test]
    fn output_helpers_expose_and_consume_json() {
        let output = ToolOutput::from(serde_json::json!({"ok": true}));
        assert_eq!(output.as_json()["ok"], true);
        assert_eq!(output["ok"], true);
        assert_eq!(output, serde_json::json!({"ok": true}));
        assert_eq!(output.into_json(), serde_json::json!({"ok": true}));
    }

    #[test]
    fn tool_input_default_is_empty_object() {
        assert_eq!(ToolInput::default().into_json(), serde_json::json!({}));
    }
}
