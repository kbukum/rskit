//! Nearest-match suggestions for "did you mean?" command-line diagnostics.
//!
//! A generic, allocation-light helper that, given an unknown token and a set of
//! valid candidates, returns the closest candidate by Damerau-style edit
//! distance. It is case-insensitive with a light prefix boost so a leading typo
//! (`Fmt` for `format`, `buld` for `build`) still resolves. The comparison is
//! bounded by a caller-supplied maximum distance so far-off tokens yield no
//! (noisy) suggestion rather than a misleading one.

/// The default maximum edit distance for a suggestion to be offered.
///
/// A distance of two catches single transpositions, one insertion plus one
/// deletion, and most realistic typos while still rejecting unrelated tokens.
pub const DEFAULT_SUGGESTION_DISTANCE: usize = 2;

/// Return the candidate nearest to `input` within [`DEFAULT_SUGGESTION_DISTANCE`].
///
/// Convenience wrapper over [`nearest_within`] using the default threshold. See
/// it for the full matching semantics.
///
/// # Examples
/// ```
/// use rskit_util::strings::nearest;
/// assert_eq!(nearest("fmt", ["format", "test", "lint"]), Some("format"));
/// assert_eq!(nearest("zzzzz", ["format", "test"]), None);
/// ```
pub fn nearest<'a, I, S>(input: &str, candidates: I) -> Option<&'a str>
where
    I: IntoIterator<Item = S>,
    S: Into<&'a str>,
{
    nearest_within(input, candidates, DEFAULT_SUGGESTION_DISTANCE)
}

/// Return the candidate nearest to `input` within `max_distance` edits.
///
/// Matching is case-insensitive. Distance is the Optimal String Alignment
/// (restricted Damerau-Levenshtein) edit distance, which counts an adjacent
/// transposition as a single edit. Among candidates within `max_distance`, the
/// one with the smallest distance wins; ties break toward a shared prefix and
/// then lexicographically, so the result is deterministic. Returns `None` when
/// no candidate is close enough, so a far-off token yields no misleading hint.
///
/// # Examples
/// ```
/// use rskit_util::strings::nearest_within;
/// assert_eq!(nearest_within("buld", ["build", "check"], 2), Some("build"));
/// assert_eq!(nearest_within("teh", ["the", "then"], 1), Some("the"));
/// assert_eq!(nearest_within("xyz", ["build"], 2), None);
/// ```
pub fn nearest_within<'a, I, S>(input: &str, candidates: I, max_distance: usize) -> Option<&'a str>
where
    I: IntoIterator<Item = S>,
    S: Into<&'a str>,
{
    let lower_input = input.to_lowercase();
    let mut best: Option<(&'a str, usize)> = None;
    for candidate in candidates {
        let candidate: &'a str = candidate.into();
        let lower_candidate = candidate.to_lowercase();
        let Some(score) = match_score(&lower_input, &lower_candidate, max_distance) else {
            continue;
        };
        if let Some((best_candidate, best_score)) = best {
            if score < best_score
                || (score == best_score && prefers(input, candidate, best_candidate, &lower_input))
            {
                best = Some((candidate, score));
            }
        } else {
            best = Some((candidate, score));
        }
    }
    best.map(|(candidate, _)| candidate)
}

/// Score a candidate against `input`, or `None` when it is not close enough.
///
/// A direct edit within `max_distance` scores by that distance. Failing that, an
/// *abbreviation* — `input` (of at least two characters) is a subsequence of a
/// candidate no more than four times its length, e.g. `fmt` → `format` — scores
/// just above an exact typo so a genuine short-hand still resolves without
/// swamping close edits.
fn match_score(input: &str, candidate: &str, max_distance: usize) -> Option<usize> {
    let distance = osa_distance(input, candidate);
    if distance <= max_distance {
        return Some(distance);
    }
    let input_len = input.chars().count();
    if input_len >= 2
        && candidate.chars().count() <= input_len.saturating_mul(4)
        && is_subsequence(input, candidate)
    {
        return Some(max_distance);
    }
    None
}

/// Whether every character of `needle` appears in `haystack` in order (an
/// abbreviation match).
fn is_subsequence(needle: &str, haystack: &str) -> bool {
    let mut chars = haystack.chars();
    needle
        .chars()
        .all(|target| chars.by_ref().any(|c| c == target))
}

/// Tie-break preference: favor a candidate that shares `input`'s leading
/// character, then the lexicographically smaller name for determinism.
fn prefers(input: &str, candidate: &str, incumbent: &str, lower_input: &str) -> bool {
    let leading = lower_input.chars().next();
    let candidate_prefix = leading.is_some_and(|c| candidate.to_lowercase().starts_with(c));
    let incumbent_prefix = leading.is_some_and(|c| incumbent.to_lowercase().starts_with(c));
    match (candidate_prefix, incumbent_prefix) {
        (true, false) => true,
        (false, true) => false,
        _ => candidate < incumbent && !input.is_empty(),
    }
}

/// Optimal String Alignment distance between two strings (adjacent
/// transpositions cost one edit; no substring may be edited more than once).
fn osa_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let cols = b.len() + 1;
    // Three rolling rows: two-before, previous, current.
    let mut two_prev = vec![0_usize; cols];
    let mut prev: Vec<usize> = (0..cols).collect();
    let mut current = vec![0_usize; cols];
    for i in 1..=a.len() {
        current[0] = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut value = (prev[j] + 1)
                .min(current[j - 1] + 1)
                .min(prev[j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                value = value.min(two_prev[j - 2] + 1);
            }
            current[j] = value;
        }
        std::mem::swap(&mut two_prev, &mut prev);
        std::mem::swap(&mut prev, &mut current);
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_SUGGESTION_DISTANCE, nearest, nearest_within, osa_distance};

    #[test]
    fn suggests_the_nearest_single_typo() {
        assert_eq!(nearest("fmt", ["format", "test", "lint"]), Some("format"));
        assert_eq!(nearest("buld", ["build", "check"]), Some("build"));
        assert_eq!(nearest("tset", ["test", "reset"]), Some("test"));
    }

    #[test]
    fn is_case_insensitive() {
        assert_eq!(nearest("FMT", ["format"]), Some("format"));
        assert_eq!(nearest("Buld", ["build"]), Some("build"));
    }

    #[test]
    fn far_off_tokens_suggest_nothing() {
        assert_eq!(nearest("zzzzzz", ["format", "build", "test"]), None);
        assert_eq!(nearest_within("xyz", ["build"], 2), None);
    }

    #[test]
    fn transposition_counts_as_one_edit() {
        assert_eq!(osa_distance("teh", "the"), 1);
        assert_eq!(nearest_within("teh", ["the", "then"], 1), Some("the"));
    }

    #[test]
    fn exact_match_is_distance_zero() {
        assert_eq!(osa_distance("build", "build"), 0);
        assert_eq!(nearest("build", ["build", "check"]), Some("build"));
    }

    #[test]
    fn prefix_shared_with_input_breaks_ties() {
        // Both `cat` and `bar` are distance 2 from `car`; the shared leading
        // `c` wins deterministically.
        assert_eq!(nearest_within("car", ["bar", "cat"], 2), Some("cat"));
    }

    #[test]
    fn empty_candidate_set_is_none() {
        let empty: [&str; 0] = [];
        assert_eq!(nearest("build", empty), None);
    }

    #[test]
    fn default_distance_is_two() {
        assert_eq!(DEFAULT_SUGGESTION_DISTANCE, 2);
    }
}
