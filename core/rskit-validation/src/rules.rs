//! Stateless validation predicates.

use validator::ValidateEmail;

/// Returns `true` if `value` is a syntactically valid, routable e-mail address.
///
/// Syntax is delegated to the `validator` crate (per-label host rules, IDN, and
/// local-part grammar) rather than a hand-rolled parser, so malformed hosts such
/// as `user@-example.com`, `user@example-.com`, and `user@exam_ple.com` are
/// rejected. On top of that syntax check this predicate layers two project
/// restrictions: control characters and whitespace are rejected outright — they
/// enable header injection when the address is later serialised into mail headers
/// or logs — and the domain must contain a dot, so bare hosts like `user@localhost`
/// are not accepted as externally routable.
pub fn validate_email(value: &str) -> bool {
    if value.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return false;
    }
    if !value.validate_email() {
        return false;
    }
    // `validate_email` already guarantees a single `@` with a syntactically valid
    // host; the dotted-domain requirement is the one project-specific restriction.
    value
        .rsplit_once('@')
        .is_some_and(|(_, domain)| domain.contains('.'))
}

/// Returns `true` if `value` is an absolute HTTP or HTTPS URL with a host.
///
/// Parsing is delegated to the `url` crate rather than a hand-rolled scanner, so
/// authorities that are not real hosts — `https://@`, `https://:443`,
/// `https://[::1`, `https:///path` — are rejected via the required non-empty host.
/// On top of that this predicate layers two project restrictions: the scheme must
/// be `http` or `https`, and control characters and whitespace are rejected so a
/// caller using this as an allow-list gate cannot be fooled by embedded newlines
/// or spaces.
pub fn validate_url(value: &str) -> bool {
    if value.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return false;
    }
    let Ok(url) = ::url::Url::parse(value) else {
        return false;
    };
    matches!(url.scheme(), "http" | "https") && url.host_str().is_some_and(|host| !host.is_empty())
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
    fn email_rejects_malformed_domain_labels() {
        assert!(!validate_email("user@-example.com"));
        assert!(!validate_email("user@example-.com"));
        assert!(!validate_email("user@exam_ple.com"));
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
        assert!(!validate_url("ftp://example.com"));
        assert!(!validate_url("example.com"));
        assert!(!validate_url("https://exa mple.com"));
        assert!(!validate_url("https://example.com\n"));
    }

    #[test]
    fn url_rejects_authorities_without_a_host() {
        assert!(!validate_url("https://@"));
        assert!(!validate_url("https://:443"));
        assert!(!validate_url("https://[::1"));
    }
}
