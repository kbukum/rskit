//! Fluent field-level validator — collects all field errors before returning.
//!
//! # Example
//!
//! ```rust
//! use rskit_validation::Validator;
//!
//! fn validate_user(name: &str, email: &str) -> rskit_errors::AppResult<()> {
//!     Validator::new()
//!         .required("name", name)
//!         .max_length("name", name, 100)
//!         .email("email", email)
//!         .validate()
//! }
//! ```

#![warn(missing_docs)]

use std::fmt::Display;

use rskit_errors::{AppError, AppResult, ErrorCode};

// ── FieldError ────────────────────────────────────────────────────────────────

/// A single field validation failure.
#[derive(Debug, Clone)]
pub struct FieldError {
    /// The name of the field that failed validation.
    pub field: String,
    /// Human-readable description of the failure.
    pub message: String,
}

// ── Validator ─────────────────────────────────────────────────────────────────

/// Fluent builder that collects field errors and converts to [`AppError`] via
/// [`Validator::validate`].
#[derive(Debug, Default)]
pub struct Validator {
    errors: Vec<FieldError>,
}

impl Validator {
    /// Create a new empty validator.
    pub fn new() -> Self {
        Self::default()
    }

    // ── String checks ─────────────────────────────────────────────────────

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
            Ok(r) if !r.is_match(value) => {
                self.add(field, format!("must match pattern {re}"));
            }
            Err(_) => {
                self.add(field, format!("invalid pattern {re}"));
            }
            _ => {}
        }
        self
    }

    // ── UUID ──────────────────────────────────────────────────────────────

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

    // ── Numeric ───────────────────────────────────────────────────────────

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

    // ── Time ──────────────────────────────────────────────────────────────

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

    // ── Enum membership ───────────────────────────────────────────────────

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

    // ── Composition ───────────────────────────────────────────────────────

    /// Add an error for `field` if `check` is `false`.
    #[must_use]
    pub fn custom(mut self, field: &str, check: bool, message: &str) -> Self {
        if !check {
            self.add(field, message);
        }
        self
    }

    // ── Terminal ──────────────────────────────────────────────────────────

    /// Returns `true` if any validation errors have been accumulated.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Returns a slice of all accumulated field errors.
    pub fn errors(&self) -> &[FieldError] {
        &self.errors
    }

    /// Consume the validator and return `Ok(())` if no errors, or an
    /// [`AppError::invalid_input`] containing all field errors.
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
        Err(AppError::new(ErrorCode::InvalidInput, detail))
    }

    // ── Internal ──────────────────────────────────────────────────────────

    fn add(&mut self, field: &str, message: impl Into<String>) {
        self.errors.push(FieldError {
            field: field.to_owned(),
            message: message.into(),
        });
    }
}

// ── Free functions ────────────────────────────────────────────────────────────

/// Returns `true` if `value` looks like a valid e-mail address.
pub fn validate_email(value: &str) -> bool {
    let parts: Vec<&str> = value.splitn(2, '@').collect();
    if parts.len() != 2 {
        return false;
    }
    let local = parts[0];
    let domain = parts[1];
    !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
}

/// Returns `true` if `value` is an absolute HTTP or HTTPS URL.
pub fn validate_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}

/// Returns `true` if `value` is a valid UUID (any version, hyphenated or not).
pub fn validate_uuid(value: &str) -> bool {
    value.parse::<uuid::Uuid>().is_ok()
}

// ── chrono re-export guard ────────────────────────────────────────────────────
// We use chrono internally but do not re-export it to avoid version conflicts.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_fails_on_empty() {
        let r = Validator::new().required("name", "").validate();
        assert!(r.is_err());
    }

    #[test]
    fn required_passes_non_empty() {
        let r = Validator::new().required("name", "Alice").validate();
        assert!(r.is_ok());
    }

    #[test]
    fn email_fails_on_invalid() {
        let r = Validator::new().email("email", "notanemail").validate();
        assert!(r.is_err());
    }

    #[test]
    fn email_passes_on_valid() {
        let r = Validator::new()
            .email("email", "user@example.com")
            .validate();
        assert!(r.is_ok());
    }

    #[test]
    fn multiple_errors_collected() {
        let v =
            Validator::new()
                .required("name", "")
                .max_length("bio", "x".repeat(201).as_str(), 200);
        assert_eq!(v.errors().len(), 2);
    }

    #[test]
    fn in_range_passes_at_boundaries() {
        let r = Validator::new().in_range("age", 0u32, 0, 120).validate();
        assert!(r.is_ok());
        let r = Validator::new().in_range("age", 120u32, 0, 120).validate();
        assert!(r.is_ok());
    }

    #[test]
    fn in_range_fails_outside() {
        let r = Validator::new().in_range("age", 121u32, 0, 120).validate();
        assert!(r.is_err());
    }

    #[test]
    fn uuid_validates() {
        assert!(validate_uuid("550e8400-e29b-41d4-a716-446655440000"));
        assert!(!validate_uuid("not-a-uuid"));
    }
}
