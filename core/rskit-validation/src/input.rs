//! Validation helpers for path and text inputs.

use rskit_errors::{AppError, AppResult};

/// Validate a required string that must not contain leading or trailing whitespace.
pub fn validate_required_trimmed(field: &str, value: &str) -> AppResult<()> {
    reject_unicode_controls(field, value)?;
    if value.trim().is_empty() {
        return Err(AppError::invalid_input(field, "is required"));
    }
    if value != value.trim() {
        return Err(AppError::invalid_input(
            field,
            "cannot contain leading or trailing whitespace",
        ));
    }
    Ok(())
}

/// Validate a required identifier that is safe to use as a path segment.
pub fn validate_path_safe_identifier(field: &str, value: &str) -> AppResult<()> {
    validate_required_trimmed(field, value)?;
    if value.contains(['/', '\\', ':']) || value == "." || value == ".." {
        return Err(AppError::invalid_input(
            field,
            "cannot contain path separators or traversal markers",
        ));
    }
    Ok(())
}

/// Validate an optional string when present and return `None` for absent values.
pub fn validate_optional_trimmed(field: &str, value: Option<String>) -> AppResult<Option<String>> {
    value
        .map(|value| {
            validate_required_trimmed(field, &value)?;
            Ok(value)
        })
        .transpose()
}

/// Validate that a path-like input cannot traverse outside its base.
pub fn validate_safe_path(path: &str) -> AppResult<()> {
    reject_unicode_controls("path", path)?;
    if path.is_empty()
        || path.starts_with('/')
        || path.starts_with('\\')
        || path.split(['/', '\\']).any(|segment| {
            segment.is_empty() || segment == "." || segment == ".." || segment.contains(':')
        })
    {
        return Err(AppError::invalid_input(
            "path",
            "path must be relative and must not contain traversal segments",
        ));
    }
    Ok(())
}

/// Reject control characters that can hide or reorder text.
pub fn reject_unicode_controls(field: &str, value: &str) -> AppResult<()> {
    for ch in value.chars() {
        if ch.is_control()
            || matches!(
                ch,
                '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{200E}' | '\u{200F}'
            )
        {
            return Err(AppError::invalid_input(
                field,
                "input contains forbidden Unicode control characters",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_validation_rejects_traversal_and_mixed_separators() {
        assert!(validate_safe_path("tenant/report.json").is_ok());
        assert!(validate_safe_path("../secret").is_err());
        assert!(validate_safe_path("tenant\\..\\secret").is_err());
        assert!(validate_safe_path("tenant/..\\secret").is_err());
    }

    #[test]
    fn unicode_validation_rejects_control_characters() {
        assert!(reject_unicode_controls("identifier", "safe-id").is_ok());
        assert!(reject_unicode_controls("identifier", "safe\u{202e}txt").is_err());
        assert!(reject_unicode_controls("identifier", "павел").is_ok());
    }

    #[test]
    fn required_trimmed_rejects_whitespace_only_and_untrimmed_values() {
        assert!(validate_required_trimmed("name", "value").is_ok());
        assert!(validate_required_trimmed("name", "   ").is_err());
        assert!(validate_required_trimmed("name", "\t").is_err());
        assert!(validate_required_trimmed("name", " value").is_err());
        assert!(validate_required_trimmed("name", "value ").is_err());
    }

    #[test]
    fn optional_trimmed_accepts_absent_and_rejects_invalid_present_values() {
        assert_eq!(validate_optional_trimmed("name", None).unwrap(), None);
        assert_eq!(
            validate_optional_trimmed("name", Some("value".to_string())).unwrap(),
            Some("value".to_string())
        );
        assert!(validate_optional_trimmed("name", Some(" value".to_string())).is_err());
        assert!(validate_optional_trimmed("name", Some("\n".to_string())).is_err());
    }

    #[test]
    fn path_safe_identifier_rejects_controls_traversal_and_separators() {
        assert!(validate_path_safe_identifier("id", "tenant_01").is_ok());
        assert!(validate_path_safe_identifier("id", ".").is_err());
        assert!(validate_path_safe_identifier("id", "..").is_err());
        assert!(validate_path_safe_identifier("id", "tenant/name").is_err());
        assert!(validate_path_safe_identifier("id", "tenant\\name").is_err());
        assert!(validate_path_safe_identifier("id", "tenant:name").is_err());
        assert!(validate_path_safe_identifier("id", "tenant\u{202e}name").is_err());
    }

    #[test]
    fn safe_path_rejects_empty_absolute_controls_traversal_and_separators() {
        assert!(validate_safe_path("tenant/report.json").is_ok());
        assert!(validate_safe_path("").is_err());
        assert!(validate_safe_path("/tenant/report.json").is_err());
        assert!(validate_safe_path("\\tenant\\report.json").is_err());
        assert!(validate_safe_path("./tenant").is_err());
        assert!(validate_safe_path("tenant//report.json").is_err());
        assert!(validate_safe_path("tenant/report:name").is_err());
        assert!(validate_safe_path("tenant/re\u{202e}port").is_err());
    }
}
