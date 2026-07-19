//! Bounded JSON/CSV record limits.
//!
//! Dataset record readers accept untrusted structured input. This module keeps byte, nesting, field,
//! array, and string limits in one place so JSON Lines and JSON array readers enforce the same policy.

use rskit_errors::{AppError, AppResult, ErrorCode};
use serde_json::Value;

pub(crate) const MAX_JSON_ARRAY_BYTES: u64 = 1024 * 1024;
pub(crate) const MAX_JSON_LINE_BYTES: usize = 1024 * 1024;
pub(crate) const MAX_CSV_RECORD_BYTES: usize = 1024 * 1024;

const MAX_JSON_DEPTH: usize = 32;
const MAX_JSON_OBJECT_FIELDS: usize = 1024;
const MAX_JSON_ARRAY_ITEMS: usize = 16_384;
const MAX_JSON_STRING_BYTES: usize = 1024 * 1024;
const MAX_JSON_FIELD_NAME_BYTES: usize = 4096;

pub(crate) fn validate_json_record(value: &Value) -> AppResult<()> {
    validate_json_value(value, 0)
}

fn validate_json_value(value: &Value, depth: usize) -> AppResult<()> {
    if depth > MAX_JSON_DEPTH {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            format!("JSON record nesting exceeds max depth {MAX_JSON_DEPTH}"),
        ));
    }

    match value {
        Value::Object(fields) => {
            if fields.len() > MAX_JSON_OBJECT_FIELDS {
                return Err(AppError::new(
                    ErrorCode::InvalidInput,
                    format!(
                        "JSON object has {} fields, exceeding max {MAX_JSON_OBJECT_FIELDS}",
                        fields.len()
                    ),
                ));
            }
            for (name, value) in fields {
                if name.len() > MAX_JSON_FIELD_NAME_BYTES {
                    return Err(AppError::new(
                        ErrorCode::InvalidInput,
                        format!(
                            "JSON field name is {} bytes, exceeding max {MAX_JSON_FIELD_NAME_BYTES}",
                            name.len()
                        ),
                    ));
                }
                validate_json_value(value, depth + 1)?;
            }
        }
        Value::Array(items) => {
            if items.len() > MAX_JSON_ARRAY_ITEMS {
                return Err(AppError::new(
                    ErrorCode::InvalidInput,
                    format!(
                        "JSON array has {} items, exceeding max {MAX_JSON_ARRAY_ITEMS}",
                        items.len()
                    ),
                ));
            }
            for item in items {
                validate_json_value(item, depth + 1)?;
            }
        }
        Value::String(value) if value.len() > MAX_JSON_STRING_BYTES => {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!(
                    "JSON string is {} bytes, exceeding max {MAX_JSON_STRING_BYTES}",
                    value.len()
                ),
            ));
        }
        Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null => {}
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validate_json_record_rejects_oversized_strings_directly() {
        let value = json!({"blob": "x".repeat(MAX_JSON_STRING_BYTES + 1)});

        let err = validate_json_record(&value).unwrap_err();

        assert_eq!(err.code(), ErrorCode::InvalidInput);
        assert!(err.to_string().contains("JSON string"));
    }

    #[test]
    fn validate_json_record_covers_object_array_depth_and_scalar_paths() {
        assert!(validate_json_record(&json!({"ok": [true, null, 1, "x"]})).is_ok());

        let too_many_fields = Value::Object(
            (0..=MAX_JSON_OBJECT_FIELDS)
                .map(|idx| (format!("f{idx}"), Value::Null))
                .collect(),
        );
        let err = validate_json_record(&too_many_fields).unwrap_err();
        assert!(err.to_string().contains("fields"));

        let too_many_items = json!({"items": vec![Value::Null; MAX_JSON_ARRAY_ITEMS + 1]});
        let err = validate_json_record(&too_many_items).unwrap_err();
        assert!(err.to_string().contains("array"));

        let deep = (0..=MAX_JSON_DEPTH + 1).fold(Value::Null, |inner, _| json!([inner]));
        let err = validate_json_record(&deep).unwrap_err();
        assert!(err.to_string().contains("nesting"));
    }
}
