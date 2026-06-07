//! Bounded JSON/CSV record limits.
//!
//! Dataset record readers accept untrusted structured input. This module keeps
//! byte, nesting, field, array, and string limits in one place so JSON Lines and
//! JSON array readers enforce the same policy.

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
