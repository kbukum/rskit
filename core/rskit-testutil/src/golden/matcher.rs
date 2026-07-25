use std::collections::BTreeMap;
use std::fmt::Write as _;

use rskit_errors::{AppError, AppResult};

use super::diff;
use super::normalize::Normalizer;

/// A comparison tier for one golden surface, strictest first.
///
/// Every tier reports a mismatch as a typed [`AppError`] carrying a
/// unified-diff or set-difference message — never a bare boolean.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Match {
    /// Byte-for-byte equality.
    Exact,
    /// Equality after the caller's [`Normalizer`] rewrites the actual output.
    Normalized(Normalizer),
    /// Positional frame + order-insensitive middle band.
    ///
    /// The first `frame_prefix` and last `frame_suffix` lines must match in
    /// place; the lines between them are compared as a multiset, so
    /// nondeterministic interleaving (parallel execution) stays green while a
    /// missing or extra line still fails.
    LineSet {
        /// Leading lines matched positionally.
        frame_prefix: usize,
        /// Trailing lines matched positionally.
        frame_suffix: usize,
    },
    /// Every non-blank expected line must appear, in order, as a substring of
    /// some actual line. For noisy output where only required markers matter.
    Subset,
}

impl Match {
    /// The text this matcher would store as the golden for `actual` — the
    /// normalized form for [`Match::Normalized`], the raw text otherwise.
    #[must_use]
    pub fn normalize(&self, actual: &str) -> String {
        match self {
            Self::Normalized(normalizer) => normalizer.apply(actual),
            _ => actual.to_owned(),
        }
    }

    /// Compare `actual` against `expected` under this tier.
    ///
    /// # Errors
    ///
    /// Returns a typed [`AppError`] describing the mismatch (a unified diff or
    /// the missing/extra lines).
    pub fn verify(&self, expected: &str, actual: &str) -> AppResult<()> {
        match self {
            Self::Exact => verify_exact(expected, actual, "exact"),
            Self::Normalized(normalizer) => {
                verify_exact(expected, &normalizer.apply(actual), "normalized")
            }
            Self::LineSet {
                frame_prefix,
                frame_suffix,
            } => verify_line_set(expected, actual, *frame_prefix, *frame_suffix),
            Self::Subset => verify_subset(expected, actual),
        }
    }
}

fn verify_exact(expected: &str, actual: &str, tier: &str) -> AppResult<()> {
    if expected == actual {
        return Ok(());
    }
    Err(AppError::conflict(format!(
        "output does not match golden ({tier}):\n{}",
        diff::unified(expected, actual)
    )))
}

fn verify_line_set(
    expected: &str,
    actual: &str,
    frame_prefix: usize,
    frame_suffix: usize,
) -> AppResult<()> {
    let expected: Vec<&str> = expected.lines().collect();
    let actual: Vec<&str> = actual.lines().collect();
    let frame = frame_prefix.saturating_add(frame_suffix);
    if expected.len() < frame {
        return Err(AppError::invalid_input(
            "golden",
            format!(
                "golden has {} lines, smaller than its line-set frame of {frame}",
                expected.len()
            ),
        ));
    }
    if actual.len() != expected.len() {
        return Err(AppError::conflict(format!(
            "output does not match golden (line-set): expected {} lines, got {}:\n{}",
            expected.len(),
            actual.len(),
            diff::unified(&expected.join("\n"), &actual.join("\n"))
        )));
    }

    // Frame lines are positional.
    let tail_start = expected.len() - frame_suffix;
    for index in (0..frame_prefix).chain(tail_start..expected.len()) {
        if expected[index] != actual[index] {
            return Err(AppError::conflict(format!(
                "output does not match golden (line-set): frame line {} differs:\n- {}\n+ {}\n",
                index + 1,
                expected[index],
                actual[index]
            )));
        }
    }

    // The middle band is a multiset: order-free, but counts must agree.
    // BTreeMap keeps the mismatch message deterministically ordered.
    let mut counts: BTreeMap<&str, i64> = BTreeMap::new();
    for line in &expected[frame_prefix..tail_start] {
        *counts.entry(line).or_default() += 1;
    }
    for line in &actual[frame_prefix..tail_start] {
        *counts.entry(line).or_default() -= 1;
    }
    let mut missing = String::new();
    let mut extra = String::new();
    for (line, count) in counts {
        let (target, repeats) = match count {
            1.. => (&mut missing, count),
            ..=-1 => (&mut extra, -count),
            0 => continue,
        };
        for _ in 0..repeats {
            let _ = writeln!(target, "  {line}");
        }
    }
    if missing.is_empty() && extra.is_empty() {
        return Ok(());
    }
    let mut message = String::from("output does not match golden (line-set):\n");
    if !missing.is_empty() {
        let _ = write!(message, "missing from actual:\n{missing}");
    }
    if !extra.is_empty() {
        let _ = write!(message, "unexpected in actual:\n{extra}");
    }
    Err(AppError::conflict(message))
}

fn verify_subset(expected: &str, actual: &str) -> AppResult<()> {
    let mut actual_lines = actual.lines();
    for required in expected.lines().filter(|line| !line.trim().is_empty()) {
        if !actual_lines.by_ref().any(|line| line.contains(required)) {
            return Err(AppError::conflict(format!(
                "output does not match golden (subset): required line not found in order:\n  {required}\n"
            )));
        }
    }
    Ok(())
}
