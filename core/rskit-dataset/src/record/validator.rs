//! Schema-backed validation for structured records.

use rskit_errors::AppResult;

use crate::schema::DatasetSchema;
use crate::validate::Validator;

use super::model::DatasetRecord;

/// Rejects records that fail a compiled [`DatasetSchema`].
pub struct SchemaValidator {
    schema: DatasetSchema,
}

impl SchemaValidator {
    /// Create a validator from a compiled schema.
    #[must_use]
    pub fn new(schema: DatasetSchema) -> Self {
        Self { schema }
    }

    /// Compile a JSON Schema and wrap it as a record validator.
    pub fn compile(schema: &serde_json::Value) -> AppResult<Self> {
        Ok(Self::new(DatasetSchema::compile(schema)?))
    }
}

impl Validator<DatasetRecord> for SchemaValidator {
    fn validate(&self, item: &DatasetRecord) -> AppResult<()> {
        self.schema.validate(&item.to_json())
    }
}

#[cfg(test)]
mod tests {
    use rskit_errors::ErrorCode;
    use serde_json::json;

    use super::*;

    #[test]
    fn schema_validator_accepts_valid_and_rejects_invalid_records() {
        let validator = SchemaValidator::compile(&json!({
            "type": "object",
            "required": ["id"],
            "properties": { "id": {"type": "string"} }
        }))
        .unwrap();

        validator
            .validate(&DatasetRecord::from_fields([("id", json!("a"))]))
            .unwrap();

        let error = validator
            .validate(&DatasetRecord::from_fields([("id", json!(1))]))
            .unwrap_err();
        assert_eq!(error.code(), ErrorCode::InvalidInput);
    }
}
