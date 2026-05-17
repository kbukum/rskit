//! Minimal domain-free utility crate for rskit.
//!
//! Foundation crates must stay cheap to depend on. Domain-owned helpers such as
//! secret masking, validation, schema handling, and config merging live in their
//! owning crates instead of this L0 crate.

#![warn(missing_docs)]

// ── Case Conversion ──────────────────────────────────────────────────────────

/// Convert a `SCREAMING_SNAKE_CASE` string to `kebab-case`.
pub fn to_kebab_case(s: &str) -> String {
    s.to_ascii_lowercase().replace('_', "-")
}

/// Convert a `SCREAMING_SNAKE_CASE` string to `Title Case`.
pub fn to_title_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    let rest: String = chars.map(|c| c.to_ascii_lowercase()).collect();
                    format!("{}{}", first.to_ascii_uppercase(), rest)
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ── String & Path Primitives ────────────────────────────────────────────────

/// Returns `true` if the string contains forbidden Unicode control characters
/// or bidirectional override characters.
pub fn has_unicode_controls(s: &str) -> bool {
    s.chars().any(|ch| {
        ch.is_control()
            || matches!(
                ch,
                '\u{202A}'..='\u{202E}'
                    | '\u{2066}'..='\u{2069}'
                    | '\u{200E}'
                    | '\u{200F}'
            )
    })
}

/// Returns `true` if the path is relative and does not contain traversal segments
/// (`..`), empty segments, or drive specifiers.
pub fn is_safe_path(path: &str) -> bool {
    if path.is_empty()
        || path.starts_with('/')
        || path.starts_with('\\')
        || path.split(['/', '\\']).any(|segment| {
            segment.is_empty() || segment == "." || segment == ".." || segment.contains(':')
        })
    {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_kebab_case() {
        assert_eq!(to_kebab_case("NOT_FOUND"), "not-found");
        assert_eq!(to_kebab_case("SERVICE_UNAVAILABLE"), "service-unavailable");
        assert_eq!(to_kebab_case("INTERNAL"), "internal");
    }

    #[test]
    fn test_to_title_case() {
        assert_eq!(to_title_case("NOT_FOUND"), "Not Found");
        assert_eq!(to_title_case("SERVICE_UNAVAILABLE"), "Service Unavailable");
        assert_eq!(to_title_case("INTERNAL"), "Internal");
    }

    #[test]
    fn test_has_unicode_controls() {
        assert!(!has_unicode_controls("safe string"));
        assert!(has_unicode_controls("string with\nnewline")); // \n is a control char
        assert!(has_unicode_controls("string with\u{202e}override"));
    }

    #[test]
    fn test_is_safe_path() {
        assert!(is_safe_path("tenant/report.json"));
        assert!(is_safe_path("a/b/c.txt"));
        assert!(!is_safe_path("/absolute/path"));
        assert!(!is_safe_path("../traversal"));
        assert!(!is_safe_path("path/../traversal"));
        assert!(!is_safe_path("path//double_slash"));
        assert!(!is_safe_path("C:\\windows"));
    }
}
