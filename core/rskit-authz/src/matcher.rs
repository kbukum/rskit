//! Wildcard permission matching helpers.

/// Return `true` when `pattern` matches `value`.
///
/// `*` matches anything. `resource:*` and `*:action` are supported.
#[must_use]
pub fn match_pattern(pattern: &str, value: &str) -> bool {
    if pattern == value || pattern == "*" || pattern == "*:*" {
        return true;
    }

    // Use split_once to avoid per-call Vec allocation on the hot auth path.
    match (pattern.split_once(':'), value.split_once(':')) {
        (Some((pp, pa)), Some((vp, va))) => wildcard_equals(pp, vp) && wildcard_equals(pa, va),
        _ => wildcard_equals(pattern, value),
    }
}

/// Return `true` when any supplied pattern matches `value`.
#[must_use]
pub fn match_any(patterns: &[String], value: &str) -> bool {
    patterns.iter().any(|pattern| match_pattern(pattern, value))
}

fn wildcard_equals(pattern: &str, value: &str) -> bool {
    pattern == "*" || pattern == value
}

#[cfg(test)]
mod tests {
    use super::{match_any, match_pattern};

    #[test]
    fn match_pattern_supports_wildcards() {
        assert!(match_pattern("*:*", "article:read"));
        assert!(match_pattern("article:*", "article:write"));
        assert!(match_pattern("*:read", "article:read"));
        assert!(match_pattern("article:read", "article:read"));
        assert!(!match_pattern("article:write", "article:read"));
    }

    #[test]
    fn match_any_returns_false_for_empty_patterns() {
        assert!(!match_any(&[], "article:read"));
    }
}
