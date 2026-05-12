//! String sanitisation and basic injection-pattern detection.
//!
//! > **Note:** [`is_safe_string`] is a defence-in-depth helper and must NOT be
//! > relied upon as a security boundary.  Always use parameterised queries,
//! > proper escaping, and framework-level protections.

use regex::Regex;
use std::sync::LazyLock;

static UNSAFE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)(--|;|'|"|<script|</script|javascript:|on\w+=|union\s+select|drop\s+table|insert\s+into|delete\s+from|update\s+.+\s+set)"#,
    )
    .expect("invalid regex")
});

/// Trim whitespace and remove Unicode control characters from `s`.
pub fn sanitize_string(s: &str) -> String {
    s.trim().chars().filter(|c| !c.is_control()).collect()
}

/// Strip surrounding quotes and trim whitespace from an environment-variable
/// value.
pub fn sanitize_env_value(s: &str) -> String {
    let s = s.trim();
    let bytes = s.as_bytes();
    let inner = if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        &s[1..s.len() - 1]
    } else {
        s
    };
    inner.trim().to_string()
}

/// Return `false` if `s` matches basic injection patterns.
///
/// This is **defence-in-depth only** — never rely on it as a security boundary.
pub fn is_safe_string(s: &str) -> bool {
    !UNSAFE_PATTERN.is_match(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_string_trims_and_strips_control() {
        assert_eq!(sanitize_string("  hello\x00world  "), "helloworld");
        assert_eq!(sanitize_string("  clean  "), "clean");
        assert_eq!(sanitize_string(""), "");
    }

    #[test]
    fn sanitize_env_value_strips_quotes() {
        assert_eq!(sanitize_env_value(r#""hello""#), "hello");
        assert_eq!(sanitize_env_value("'hello'"), "hello");
        assert_eq!(sanitize_env_value("  \"spaced\"  "), "spaced");
        assert_eq!(sanitize_env_value("no_quotes"), "no_quotes");
        assert_eq!(sanitize_env_value("\"mismatched'"), "\"mismatched'");
    }

    #[test]
    fn is_safe_string_detects_injection() {
        assert!(is_safe_string("normal input"));
        assert!(!is_safe_string("1; DROP TABLE users"));
        assert!(!is_safe_string("<script>alert(1)</script>"));
        assert!(!is_safe_string("javascript:alert(1)"));
        assert!(!is_safe_string("' OR 1=1 --"));
        assert!(!is_safe_string("UNION SELECT * FROM users"));
    }

    #[test]
    fn sanitize_string_unicode() {
        assert_eq!(sanitize_string("café ☕"), "café ☕");
        assert_eq!(sanitize_string("emoji 🎉"), "emoji 🎉");
    }
}
