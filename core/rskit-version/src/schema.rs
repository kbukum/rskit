//! Schema-version compatibility gating.
//!
//! A small, general-purpose helper for versioned documents (config files,
//! manifests, on-disk formats) that declare a `schema` field and must reject any
//! version the current code does not understand. This is distinct from
//! [`crate::semver`] (which parses semantic-version strings): a schema version is
//! typically a small monotonic integer, so the gate is generic over any
//! `Copy + Eq + Display` value.

use std::fmt::Display;

use rskit_errors::{AppError, AppResult};

/// Return the configured schema version, or the supported default when absent,
/// rejecting any value the current code does not support.
///
/// `field` names the schema field for error reporting. A `None` `configured`
/// value defaults to `supported`. A `configured` value that differs from
/// `supported` is a typed [`AppError::invalid_input`] — the caller cannot safely
/// interpret a document written for a different schema version.
///
/// # Errors
///
/// Returns [`AppError::invalid_input`] when `configured` is present and not equal
/// to `supported`.
///
/// # Examples
///
/// ```
/// use rskit_version::schema::supported_schema;
///
/// // Absent value defaults to the supported version.
/// assert_eq!(supported_schema("schema", None, 1).unwrap(), 1);
/// // A matching value is accepted.
/// assert_eq!(supported_schema("schema", Some(1), 1).unwrap(), 1);
/// // A mismatching value is rejected.
/// assert!(supported_schema("schema", Some(2), 1).is_err());
/// ```
pub fn supported_schema<T>(field: &str, configured: Option<T>, supported: T) -> AppResult<T>
where
    T: Copy + Eq + Display,
{
    let schema = configured.unwrap_or(supported);
    if schema != supported {
        return Err(AppError::invalid_input(
            field,
            format!("unsupported schema {schema}; supported schema is {supported}"),
        ));
    }
    Ok(schema)
}

#[cfg(test)]
mod tests {
    use rskit_errors::ErrorCode;

    use super::supported_schema;

    #[test]
    fn defaults_absent_value() {
        assert_eq!(supported_schema("schema", None, 1).unwrap(), 1);
    }

    #[test]
    fn accepts_supported_value() {
        assert_eq!(supported_schema("schema", Some(1), 1).unwrap(), 1);
    }

    #[test]
    fn rejects_unsupported_value() {
        let error = supported_schema("schema", Some(2), 1).unwrap_err();

        assert_eq!(error.code(), ErrorCode::InvalidInput);
        assert!(error.message().contains("unsupported schema 2"));
    }
}
