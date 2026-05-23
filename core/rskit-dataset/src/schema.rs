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
