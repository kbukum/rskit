//! Resolve a user shorthand to the unique candidate it names.
//!
//! A recurring command-line need: the user types a short token (`core`,
//! `billing`) and the tool must map it to exactly one item from a known set,
//! erroring rather than guessing when the token is ambiguous. This mirrors how
//! `git`, `cargo -p`, and `kubectl` accept a shorthand only when it is
//! unambiguous. The helper is error-policy-neutral — it distinguishes "no match"
//! (returned as `None`) from "several matches" (returned as a typed
//! [`Ambiguity`]) so the caller wraps each outcome in whatever domain error
//! carries the right context.

use std::fmt;

/// The candidates a shorthand matched when more than one qualified.
///
/// Carries the offending `input` and every matching candidate in the order they
/// were encountered, so the caller can render an actionable "did you mean one of
/// …?" diagnostic. Held by value so it outlives the borrowed candidate iterator.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Ambiguity<T> {
    /// The shorthand that matched more than one candidate.
    pub input: String,
    /// Every candidate whose key equalled `input`, in encounter order.
    pub matches: Vec<T>,
}

impl<T: fmt::Display> fmt::Display for Ambiguity<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "'{}' is ambiguous; matched ", self.input)?;
        for (index, candidate) in self.matches.iter().enumerate() {
            if index > 0 {
                formatter.write_str(", ")?;
            }
            write!(formatter, "{candidate}")?;
        }
        Ok(())
    }
}

/// Resolve `input` to the unique candidate whose key equals it.
///
/// `key_of` projects each candidate to the string compared against `input`.
/// Returns `Ok(Some(candidate))` when exactly one candidate's key equals `input`,
/// `Ok(None)` when none do (the caller decides whether an empty match is an error
/// and how to list the alternatives), and an [`Ambiguity`] error listing the
/// matches when two or more share the key. Comparison is exact and
/// case-sensitive; callers wanting fuzzy resolution use
/// [`nearest`](super::nearest) instead.
///
/// # Examples
/// ```
/// use rskit_util::strings::{resolve_unique, Ambiguity};
///
/// fn tail<'a>(candidate: &'a &str) -> &'a str {
///     candidate.rsplit('/').next().unwrap_or(candidate)
/// }
/// let candidates = ["service/user", "job/report", "service/report"];
///
/// assert_eq!(resolve_unique("user", candidates, tail), Ok(Some("service/user")));
/// assert_eq!(resolve_unique("missing", candidates, tail), Ok(None));
/// assert!(matches!(resolve_unique("report", candidates, tail), Err(Ambiguity { .. })));
/// ```
///
/// # Errors
/// Returns [`Ambiguity`] when `input` matches more than one candidate.
pub fn resolve_unique<T, I, F>(
    input: &str,
    candidates: I,
    key_of: F,
) -> Result<Option<T>, Ambiguity<T>>
where
    I: IntoIterator<Item = T>,
    F: Fn(&T) -> &str,
{
    let mut matches: Vec<T> = candidates
        .into_iter()
        .filter(|candidate| key_of(candidate) == input)
        .collect();
    match matches.len() {
        0 => Ok(None),
        1 => Ok(matches.pop()),
        _ => Err(Ambiguity {
            input: input.to_owned(),
            matches,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{Ambiguity, resolve_unique};

    fn tail<'a>(candidate: &'a &str) -> &'a str {
        candidate.rsplit('/').next().unwrap_or(candidate)
    }

    #[test]
    fn unique_match_resolves() {
        let candidates = ["service/user", "job/report"];
        assert_eq!(
            resolve_unique("user", candidates, tail),
            Ok(Some("service/user"))
        );
    }

    #[test]
    fn no_match_is_none() {
        let candidates = ["service/user", "job/report"];
        assert_eq!(resolve_unique("ghost", candidates, tail), Ok(None));
    }

    #[test]
    fn several_matches_are_ambiguous_in_order() {
        let candidates = ["service/report", "job/report"];
        let error = resolve_unique("report", candidates, tail).unwrap_err();
        assert_eq!(
            error,
            Ambiguity {
                input: "report".to_owned(),
                matches: vec!["service/report", "job/report"],
            }
        );
    }

    #[test]
    fn ambiguity_renders_the_candidates() {
        let candidates = ["service/report", "job/report"];
        let error = resolve_unique("report", candidates, tail).unwrap_err();
        assert_eq!(
            error.to_string(),
            "'report' is ambiguous; matched service/report, job/report"
        );
    }

    #[test]
    fn comparison_is_case_sensitive() {
        let candidates = ["service/User"];
        assert_eq!(resolve_unique("user", candidates, tail), Ok(None));
    }
}
