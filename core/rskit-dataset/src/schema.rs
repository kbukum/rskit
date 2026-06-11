//! Schema validation delegated to `rskit-schema`.

use rskit_errors::{AppError, AppResult, ErrorCode};

/// Compiled dataset record schema.
pub struct DatasetSchema {
    compiled: rskit_schema::CompiledSchema,
}

impl DatasetSchema {
    /// Compile a JSON Schema for repeated record validation.
    pub fn compile(schema: &serde_json::Value) -> AppResult<Self> {
        rskit_schema::compile(schema).map(|compiled| Self { compiled })
    }

    /// Validate one structured record against this schema.
    pub fn validate(&self, record: &serde_json::Value) -> AppResult<()> {
        let result = self.compiled.validate(record);
        if result.valid {
            return Ok(());
        }
        let detail = result
            .errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ");
        Err(AppError::new(
            ErrorCode::InvalidInput,
            format!("dataset record failed schema validation: {detail}"),
        ))
    }
}

/// Validate one record against a JSON Schema.
pub fn validate_record(schema: &DatasetSchema, record: &serde_json::Value) -> AppResult<()> {
    schema.validate(record)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn dataset_schema_validates_records_and_reports_failures() {
        let schema = DatasetSchema::compile(&json!({
            "type": "object",
            "required": ["id"],
            "properties": {
                "id": {"type": "string"},
                "score": {"type": "number"}
            }
        }))
        .unwrap();

        validate_record(&schema, &json!({"id": "a", "score": 1.0})).unwrap();

        let err = validate_record(&schema, &json!({"score": "bad"})).unwrap_err();
        assert_eq!(err.code(), ErrorCode::InvalidInput);
        assert!(
            err.to_string()
                .contains("dataset record failed schema validation")
        );
    }
}
