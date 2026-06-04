//! Stateless validation predicates.

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
