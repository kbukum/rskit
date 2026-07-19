//! Fluent validation builder.

use std::fmt::Display;

use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::{FieldError, validate_email, validate_url, validate_uuid};

/// Fluent builder that collects field errors and converts to [`AppError`] via [`Validator::validate`].
#[derive(Debug, Default)]
pub struct Validator {
    errors: Vec<FieldError>,
}

impl Validator {
    /// Create a new empty validator.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Fail if `value` is empty or whitespace-only.
    #[must_use]
    pub fn required(mut self, field: &str, value: &str) -> Self {
        if value.trim().is_empty() {
            self.add(field, "is required");
        }
        self
    }

    /// Fail if `value` has fewer than `min` characters.
    #[must_use]
    pub fn min_length(mut self, field: &str, value: &str, min: usize) -> Self {
        if value.chars().count() < min {
            self.add(field, format!("must be at least {min} characters"));
        }
        self
    }

    /// Fail if `value` exceeds `max` characters.
    #[must_use]
    pub fn max_length(mut self, field: &str, value: &str, max: usize) -> Self {
        if value.chars().count() > max {
            self.add(field, format!("must be at most {max} characters"));
        }
        self
    }

    /// Fail if `value` is not a valid e-mail address.
    #[must_use]
    pub fn email(mut self, field: &str, value: &str) -> Self {
        if !validate_email(value) {
            self.add(field, "must be a valid email address");
        }
        self
    }

    /// Fail if `value` is not a valid HTTP/HTTPS URL.
    #[must_use]
    pub fn url(mut self, field: &str, value: &str) -> Self {
        if !validate_url(value) {
            self.add(field, "must be a valid URL");
        }
        self
    }

    /// Fail if `value` does not match the regular expression `re`.
    #[must_use]
    pub fn pattern(mut self, field: &str, value: &str, re: &str) -> Self {
        match regex::Regex::new(re) {
            Ok(regex) => {
                if !regex.is_match(value) {
                    self.add(field, format!("must match pattern {re}"));
                }
            }
            Err(err) => self.add(field, format!("invalid pattern: {err}")),
        }
        self
    }

    /// Fail if `value` is not a valid UUID string.
    #[must_use]
    pub fn required_uuid(mut self, field: &str, value: &str) -> Self {
        if !validate_uuid(value) {
            self.add(field, "must be a valid UUID");
        }
        self
    }

    /// Fail if `value` is `Some` but not a valid UUID string.
    #[must_use]
    pub fn optional_uuid(mut self, field: &str, value: Option<&str>) -> Self {
        if let Some(v) = value
            && !validate_uuid(v)
        {
            self.add(field, "must be a valid UUID");
        }
        self
    }

    /// Fail if `value` is outside the inclusive range `[min, max]`.
    #[must_use]
    pub fn in_range<T: PartialOrd + Display>(
        mut self,
        field: &str,
        value: T,
        min: T,
        max: T,
    ) -> Self {
        if value < min || value > max {
            self.add(field, format!("must be between {min} and {max}"));
        }
        self
    }

    /// Fail if `value` is below `min`.
    #[must_use]
    pub fn min_value<T: PartialOrd + Display>(mut self, field: &str, value: T, min: T) -> Self {
        if value < min {
            self.add(field, format!("must be at least {min}"));
        }
        self
    }

    /// Fail if `value` is above `max`.
    #[must_use]
    pub fn max_value<T: PartialOrd + Display>(mut self, field: &str, value: T, max: T) -> Self {
        if value > max {
            self.add(field, format!("must be {max} or less"));
        }
        self
    }

    /// Fail if `value` (ISO-8601 datetime string) is not before `deadline`.
    #[must_use]
    pub fn before(mut self, field: &str, value: &str, deadline: &str) -> Self {
        match (
            chrono::DateTime::parse_from_rfc3339(value),
            chrono::DateTime::parse_from_rfc3339(deadline),
        ) {
            (Ok(v), Ok(d)) if v >= d => {
                self.add(field, format!("must be before {deadline}"));
            }
            (Err(_), _) => self.add(field, "must be a valid datetime"),
            _ => {}
        }
        self
    }

    /// Fail if `value` (ISO-8601 datetime string) is not after `floor`.
    #[must_use]
    pub fn after(mut self, field: &str, value: &str, floor: &str) -> Self {
        match (
            chrono::DateTime::parse_from_rfc3339(value),
            chrono::DateTime::parse_from_rfc3339(floor),
        ) {
            (Ok(v), Ok(f)) if v <= f => {
                self.add(field, format!("must be after {floor}"));
            }
            (Err(_), _) => self.add(field, "must be a valid datetime"),
            _ => {}
        }
        self
    }

    /// Fail if `value` is not in `allowed`.
    #[must_use]
    pub fn one_of<T: PartialEq + Display>(mut self, field: &str, value: &T, allowed: &[T]) -> Self {
        if !allowed.iter().any(|a| a == value) {
            let list = allowed
                .iter()
                .map(|a| a.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            self.add(field, format!("must be one of: {list}"));
        }
        self
    }

    /// Add an error for `field` if `condition` is `false`.
    #[must_use]
    pub fn custom(mut self, condition: bool, field: &str, message: &str) -> Self {
        if !condition {
            self.add(field, message);
        }
        self
    }

    /// Returns `true` if any validation errors have been accumulated.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Returns a slice of all accumulated field errors.
    #[must_use]
    pub fn errors(&self) -> &[FieldError] {
        &self.errors
    }

    /// Consume the validator and return `Ok(())` if no errors,
    /// or an [`AppError::invalid_input`] containing all field errors.
    pub fn validate(self) -> AppResult<()> {
        if self.errors.is_empty() {
            return Ok(());
        }
        let detail = self
            .errors
            .iter()
            .map(|e| format!("{}: {}", e.field, e.message))
            .collect::<Vec<_>>()
            .join("; ");
        let fields_json =
            serde_json::to_value(&self.errors).unwrap_or_else(|_| serde_json::Value::Array(vec![]));
        Err(AppError::new(ErrorCode::InvalidInput, detail).with_detail("fields", fields_json))
    }

    fn add(&mut self, field: &str, message: impl Into<String>) {
        self.errors.push(FieldError {
            field: field.to_owned(),
            message: message.into(),
        });
    }
}
