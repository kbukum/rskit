use rskit_errors::{AppError, AppResult, ErrorCode};
use serde_json::Value;

/// A single validation error with a JSON-pointer path.
#[derive(Debug, Clone)]
pub struct ValidationError {
    /// JSON Pointer path to the failing value.
    pub path: String,
    /// Human-readable validation failure message.
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
    /// Whether the value satisfied the schema.
    pub valid: bool,
    /// Validation failures. Empty when `valid` is true.
    pub errors: Vec<ValidationError>,
}

/// Reusable compiled JSON Schema validator.
pub struct CompiledSchema {
    validator: jsonschema::Validator,
}

impl CompiledSchema {
    /// Validate a JSON value against this compiled schema.
    pub fn validate(&self, value: &Value) -> ValidationResult {
        validation_result(&self.validator, value)
    }
}

/// Compile a JSON Schema once for repeated validation.
pub fn compile(schema: &Value) -> AppResult<CompiledSchema> {
    jsonschema::validator_for(schema)
        .map(|validator| CompiledSchema { validator })
        .map_err(|err| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("invalid JSON Schema: {err}"),
            )
            .with_cause(err)
        })
}

/// Validate a JSON value against a JSON Schema.
pub fn validate(schema: &Value, value: &Value) -> ValidationResult {
    let validator = match compile(schema) {
        Ok(validator) => validator,
        Err(err) => {
            return ValidationResult {
                valid: false,
                errors: vec![ValidationError {
                    path: String::new(),
                    message: err.message().to_owned(),
                }],
            };
        }
    };

    validator.validate(value)
}

fn validation_result(validator: &jsonschema::Validator, value: &Value) -> ValidationResult {
    let errors = validator
        .iter_errors(value)
        .map(|err| ValidationError {
            path: err.instance_path().to_string(),
            message: err.to_string(),
        })
        .collect::<Vec<_>>();

    ValidationResult {
        valid: errors.is_empty(),
        errors,
    }
}

/// Validate structured model output against a JSON Schema 2020-12-compatible schema subset.
pub fn validate_structured_output(schema: &Value, value: &Value) -> ValidationResult {
    validate(schema, value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
