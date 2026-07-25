//! Minimal line diff used by mismatch messages.

/// Above this many cells the LCS table is not worth building; fall back to a
/// first-divergence report so pathological outputs stay cheap.
const MAX_LCS_CELLS: usize = 1_000_000;

/// Render a unified-style line diff of `expected` vs `actual`.
///
/// Common lines are prefixed with two spaces, expected-only lines with `- `,
/// actual-only lines with `+ `. For very large inputs the diff degrades to a
/// report of the first diverging line.
pub(crate) fn unified(expected: &str, actual: &str) -> String {
    // `lines()` swallows a final newline, so inputs that differ only there
    // would otherwise render a "diff" with no differences at all.
    let expected_lines: Vec<&str> = expected.lines().collect();
    let actual_lines: Vec<&str> = actual.lines().collect();
    if expected_lines == actual_lines {
        return String::from("(inputs differ only in trailing newline or line endings)\n");
    }
    let expected = expected_lines;
    let actual = actual_lines;

    if expected.len().saturating_mul(actual.len()) > MAX_LCS_CELLS {
        return first_divergence(&expected, &actual);
    }

    // Classic LCS table over lines; backtrack to interleave common/-/+ lines.
    let rows = expected.len() + 1;
    let cols = actual.len() + 1;
    let mut lcs = vec![0_usize; rows * cols];
    for (i, expected_line) in expected.iter().enumerate().rev() {
        for (j, actual_line) in actual.iter().enumerate().rev() {
            lcs[i * cols + j] = if expected_line == actual_line {
                lcs[(i + 1) * cols + j + 1] + 1
            } else {
                lcs[(i + 1) * cols + j].max(lcs[i * cols + j + 1])
            };
        }
    }

    let mut out = String::new();
    let (mut i, mut j) = (0, 0);
    while i < expected.len() && j < actual.len() {
        if expected[i] == actual[j] {
            render_line(&mut out, "  ", expected[i]);
            i += 1;
            j += 1;
        } else if lcs[(i + 1) * cols + j] >= lcs[i * cols + j + 1] {
            render_line(&mut out, "- ", expected[i]);
            i += 1;
        } else {
            render_line(&mut out, "+ ", actual[j]);
            j += 1;
        }
    }
    for line in &expected[i..] {
        render_line(&mut out, "- ", line);
    }
    for line in &actual[j..] {
        render_line(&mut out, "+ ", line);
    }
    out
}

fn render_line(out: &mut String, prefix: &str, line: &str) {
    out.push_str(prefix);
    out.push_str(line);
    out.push('\n');
}

fn first_divergence(expected: &[&str], actual: &[&str]) -> String {
    let index = expected
        .iter()
        .zip(actual.iter())
        .position(|(e, a)| e != a)
        .unwrap_or_else(|| expected.len().min(actual.len()));
    let expected_line = expected.get(index).copied().unwrap_or("<end of input>");
    let actual_line = actual.get(index).copied().unwrap_or("<end of input>");
    format!(
        "first divergence at line {}:\n- {expected_line}\n+ {actual_line}\n",
        index + 1
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_common_removed_and_added_lines() {
        let diff = unified("a\nb\nc\n", "a\nx\nc\n");
        assert_eq!(diff, "  a\n- b\n+ x\n  c\n");
    }

    #[test]
    fn handles_pure_insertions_and_deletions() {
        assert_eq!(unified("a\n", "a\nb\n"), "  a\n+ b\n");
        assert_eq!(unified("a\nb\n", "a\n"), "  a\n- b\n");
    }
}
