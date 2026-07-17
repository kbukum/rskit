/// Step focus up by one, wrapping to the last row.
pub(crate) const fn focus_up(cursor: usize, len: usize) -> usize {
    if cursor == 0 { len - 1 } else { cursor - 1 }
}

/// Step focus down by one, wrapping to the first row.
pub(crate) const fn focus_down(cursor: usize, len: usize) -> usize {
    if cursor + 1 >= len { 0 } else { cursor + 1 }
}

/// Parse a one-based choice number into a zero-based index within `0..len`.
pub(crate) fn parse_index(input: &str, len: usize) -> Option<usize> {
    let number: usize = input.parse().ok()?;
    (1..=len).contains(&number).then(|| number - 1)
}
