//! JSON Schema compilation and validation.

use crate::ValidationLimits;
#[cfg(feature = "validation")]
use crate::limits::check_json_limits;
#[cfg(feature = "validation")]
use rskit_errors::{AppError, AppResult, ErrorCode};
#[cfg(feature = "validation")]
use serde_json::Value;

/// Options for schema compilation and value validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ValidationOptions {
    /// Structural limits applied to schemas and values before validation.
    pub limits: ValidationLimits,
}

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

impl ValidationResult {
    /// Return a valid result with no errors.
    #[must_use]
    pub const fn valid() -> Self {
        Self {
            valid: true,
            errors: Vec::new(),
        }
    }

    /// Return an invalid result containing one root-level error.
    #[must_use]
    pub fn invalid(message: impl Into<String>) -> Self {
        Self {
            valid: false,
            errors: vec![ValidationError {
                path: String::new(),
                message: message.into(),
            }],
        }
    }
}

/// Reusable compiled JSON Schema validator.
#[cfg(feature = "validation")]
pub struct CompiledSchema {
    validator: jsonschema::Validator,
    options: ValidationOptions,
}

#[cfg(feature = "validation")]
impl CompiledSchema {
    /// Validate a JSON value against this compiled schema.
    pub fn validate(&self, value: &Value) -> ValidationResult {
        match self.try_validate(value) {
            Ok(result) => result,
            Err(err) => ValidationResult::invalid(err.message().to_owned()),
        }
    }

    /// Validate a JSON value and surface structural-limit failures as [`AppError`].
    pub fn try_validate(&self, value: &Value) -> AppResult<ValidationResult> {
        check_json_limits("value", value, self.options.limits)?;
        Ok(validation_result(&self.validator, value))
    }
}

/// Compile a JSON Schema once for repeated validation.
#[cfg(feature = "validation")]
pub fn compile(schema: &Value) -> AppResult<CompiledSchema> {
    compile_with_options(schema, ValidationOptions::default())
}

/// Compile a JSON Schema once with explicit validation options.
#[cfg(feature = "validation")]
pub fn compile_with_options(
    schema: &Value,
    options: ValidationOptions,
) -> AppResult<CompiledSchema> {
    check_json_limits("schema", schema, options.limits)?;
    jsonschema::validator_for(schema)
        .map(|validator| CompiledSchema { validator, options })
        .map_err(|err| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("invalid JSON Schema: {err}"),
            )
            .with_cause(err)
        })
}

/// Validate a JSON value against a JSON Schema.
///
/// Invalid schemas and structural-limit violations are converted to a
/// `ValidationResult` for compatibility with callers that expect validation
/// failures rather than hard errors.
#[cfg(feature = "validation")]
pub fn validate(schema: &Value, value: &Value) -> ValidationResult {
    match validate_with_options(schema, value, ValidationOptions::default()) {
        Ok(result) => result,
        Err(err) => ValidationResult::invalid(err.message().to_owned()),
    }
}

/// Validate a JSON value against a JSON Schema and return hard setup errors.
#[cfg(feature = "validation")]
pub fn validate_with_options(
    schema: &Value,
    value: &Value,
    options: ValidationOptions,
) -> AppResult<ValidationResult> {
    let validator = compile_with_options(schema, options)?;
    validator.try_validate(value)
}

#[cfg(feature = "validation")]
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
#[cfg(feature = "validation")]
pub fn validate_structured_output(schema: &Value, value: &Value) -> ValidationResult {
    validate(schema, value)
}
