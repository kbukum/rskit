//! Shell-style glob matching for single string segments.
//!
//! A minimal, dependency-free matcher for the two wildcards every shell shares:
//! `*` matches any run of characters (including none) and `?` matches exactly one
//! character. It operates on whole strings with no path or separator semantics —
//! callers that want per-segment matching split first and match each segment — so
//! it composes into identifiers, names, topic segments, or any other flat token
//! set without smuggling in filesystem assumptions.
//!
//! Matching is Unicode-`char`-oriented and case-sensitive: `?` matches exactly one
//! Unicode scalar value, not one byte. The two-pointer algorithm uses constant
//! backtracking state — worst case `O(pattern_len * text_len)`, never the
//! exponential blow-up a naive recursive matcher can exhibit — so it stays bounded
//! on untrusted input.

/// Whether `pattern` matches `text`, treating `*` and `?` as wildcards.
///
/// `*` matches any sequence of characters (including the empty sequence) and `?`
/// matches exactly one character; every other character matches itself. A pattern
/// with no wildcards matches only its literal equal, so `glob_match("core",
/// "core")` is `true` and `glob_match("core", "cores")` is `false`.
///
/// # Examples
/// ```
/// use rskit_util::glob::glob_match;
/// assert!(glob_match("item-*", "item-core"));
/// assert!(glob_match("*-core", "item-core"));
/// assert!(glob_match("co?e", "core"));
/// assert!(!glob_match("service:*", "tenant:api"));
/// ```
#[must_use]
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    wildcard(&pattern, &text)
}

/// Whether `pattern` contains any wildcard metacharacter (`*` or `?`).
///
/// Lets callers decide whether a token is an exact literal or a glob before
/// deciding how to resolve it (e.g. treating an exact miss as an error but a glob
/// miss as an empty set).
///
/// # Examples
/// ```
/// use rskit_util::glob::has_wildcard;
/// assert!(has_wildcard("item-*"));
/// assert!(!has_wildcard("core"));
/// ```
#[must_use]
pub fn has_wildcard(pattern: &str) -> bool {
    pattern.chars().any(|ch| ch == '*' || ch == '?')
}

/// A compiled glob pattern that can be matched against many candidates.
///
/// Parses the source pattern into its character sequence once and remembers
/// whether it is a plain literal, so reusing one pattern across a candidate set
/// neither re-scans for wildcards nor re-parses the pattern per call — only each
/// candidate's own characters are walked. Matching semantics are identical to
/// [`glob_match`].
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Glob {
    pattern: String,
    chars: Vec<char>,
    literal: bool,
}

impl Glob {
    /// Compile `pattern` into a reusable matcher.
    #[must_use]
    pub fn new(pattern: impl Into<String>) -> Self {
        let pattern = pattern.into();
        let chars: Vec<char> = pattern.chars().collect();
        let literal = !chars.iter().any(|&ch| ch == '*' || ch == '?');
        Self {
            pattern,
            chars,
            literal,
        }
    }

    /// Whether the pattern is a plain literal with no wildcards.
    #[must_use]
    pub const fn is_literal(&self) -> bool {
        self.literal
    }

    /// The source pattern string.
    #[must_use]
    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    /// Whether this pattern matches `text`.
    #[must_use]
    pub fn matches(&self, text: &str) -> bool {
        if self.literal {
            self.pattern == text
        } else {
            let text: Vec<char> = text.chars().collect();
            wildcard(&self.chars, &text)
        }
    }
}

/// Two-pointer wildcard matcher supporting `*` (any run) and `?` (one char).
///
/// Uses constant backtracking state — worst case `O(pattern_len * text_len)`,
/// never exponential: on a mismatch after a `*` it rewinds the text pointer one
/// character past the last `*` rather than recursing.
fn wildcard(pattern: &[char], text: &[char]) -> bool {
    let (mut p, mut t) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            mark = t;
            p += 1;
        } else if let Some(star_pos) = star {
            p = star_pos + 1;
            mark += 1;
            t = mark;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    p == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::{Glob, glob_match, has_wildcard};

    #[test]
    fn literal_matches_only_equal() {
        assert!(glob_match("core", "core"));
        assert!(!glob_match("core", "cores"));
        assert!(!glob_match("core", "cor"));
    }

    #[test]
    fn star_matches_any_run_including_empty() {
        assert!(glob_match("item-*", "item-core"));
        assert!(glob_match("item-*", "item-"));
        assert!(glob_match("*-core", "item-core"));
        assert!(glob_match("*", "anything"));
        assert!(glob_match("*", ""));
    }

    #[test]
    fn question_matches_exactly_one() {
        assert!(glob_match("co?e", "core"));
        assert!(!glob_match("co?e", "coe"));
        assert!(!glob_match("co?e", "coree"));
    }

    #[test]
    fn wildcards_operate_on_whole_chars() {
        assert!(glob_match("caf?", "café"));
        assert!(glob_match("*é", "café"));
        assert!(glob_match("caf?", "cafe"));
        assert!(!glob_match("caf?", "café!"));
    }

    #[test]
    fn segments_do_not_leak_across_a_qualifier() {
        assert!(!glob_match("service:*", "tenant:api"));
        assert!(glob_match("service:*", "service:api"));
    }

    #[test]
    fn reports_wildcard_presence() {
        assert!(has_wildcard("item-*"));
        assert!(has_wildcard("co?e"));
        assert!(!has_wildcard("core"));
    }

    #[test]
    fn compiled_glob_tracks_literalness() {
        let literal = Glob::new("core");
        assert!(literal.is_literal());
        assert!(literal.matches("core"));
        assert!(!literal.matches("cores"));

        let pattern = Glob::new("item-*");
        assert!(!pattern.is_literal());
        assert!(pattern.matches("item-core"));
        assert_eq!(pattern.pattern(), "item-*");
    }

    #[test]
    fn trailing_stars_after_exhausted_text() {
        assert!(glob_match("a**", "a"));
        assert!(glob_match("a*b*", "ab"));
    }
}
