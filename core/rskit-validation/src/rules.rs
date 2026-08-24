//! Stateless validation predicates.

/// Returns `true` if `value` is a syntactically valid e-mail address.
///
/// Requires exactly one `@`, a non-empty local part, and a domain that contains
/// a dot without a leading, trailing, or empty label. Control characters and
/// whitespace are rejected outright — they enable header injection when the
/// address is later serialised into mail headers or logs.
pub fn validate_email(value: &str) -> bool {
    if value.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return false;
    }
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    // A second `@` lands in `domain` because `split_once` splits on the first.
    if local.is_empty() || domain.contains('@') {
        return false;
    }
    domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !domain.contains("..")
}

/// Returns `true` if `value` is an absolute HTTP or HTTPS URL with a host.
///
/// A bare scheme (`https://`) is rejected: an `http`/`https` prefix alone does
/// not make a URL. Control characters and whitespace are rejected so a caller
/// using this as an allow-list gate cannot be fooled by embedded newlines or
/// spaces.
pub fn validate_url(value: &str) -> bool {
    if value.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return false;
    }
    let Some(authority) = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
    else {
        return false;
    };
    // The host ends at the first path, query, or fragment delimiter; it must be
    // non-empty.
    authority
        .split(['/', '?', '#'])
        .next()
        .is_some_and(|host| !host.is_empty())
}

/// Returns `true` if `value` is a valid UUID (any version, hyphenated or not).
pub fn validate_uuid(value: &str) -> bool {
    value.parse::<uuid::Uuid>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::{validate_email, validate_url};

    #[test]
    fn email_accepts_well_formed_addresses() {
        assert!(validate_email("user@example.com"));
        assert!(validate_email("a.b+c@sub.example.co.uk"));
    }

    #[test]
    fn email_rejects_malformed_and_unsafe_addresses() {
        assert!(!validate_email(""));
        assert!(!validate_email("no-at-sign"));
        assert!(!validate_email("@example.com"));
        assert!(!validate_email("user@"));
        assert!(!validate_email("user@nodot"));
        assert!(!validate_email("a@b@c.com"));
        assert!(!validate_email("user@.example.com"));
        assert!(!validate_email("user@example.com."));
        assert!(!validate_email("user@ex..ample.com"));
        assert!(!validate_email("user\n@example.com"));
        assert!(!validate_email("user @example.com"));
    }

    #[test]
    fn url_accepts_absolute_http_and_https_urls() {
        assert!(validate_url("http://example.com"));
        assert!(validate_url("https://example.com/path?q=1#frag"));
    }

    #[test]
    fn url_rejects_bare_schemes_and_unsafe_values() {
        assert!(!validate_url("https://"));
        assert!(!validate_url("http://"));
        assert!(!validate_url("https:///path"));
        assert!(!validate_url("ftp://example.com"));
        assert!(!validate_url("example.com"));
        assert!(!validate_url("https://exa mple.com"));
        assert!(!validate_url("https://example.com\n"));
    }
}
