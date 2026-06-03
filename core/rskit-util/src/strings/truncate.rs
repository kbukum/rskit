/// Safely truncates a string to a UTF-8-safe prefix sized for [`truncate_owned`].
///
/// The returned prefix is at most `max_bytes - 3` bytes when truncation is needed, reserving
/// space for the ellipsis that [`truncate_owned`] appends.
///
/// Use [`truncate_owned`] if you need the truncated string with an appended ellipsis (`...`).
/// # Examples
///
/// ```
/// use rskit_util::strings::truncate;
/// assert_eq!(truncate("hello world", 8), "hello");
/// assert_eq!(truncate("hello", 10), "hello");
/// assert_eq!(truncate("🦀🦀🦀🦀", 8), "🦀");
/// ```
pub fn truncate(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }

    // `truncate_owned` appends "..." (3 bytes), so `truncate` returns at most max_bytes - 3 bytes.
    if max_bytes <= 3 {
        return &s[..0]; // too short to include any prefix before the ellipsis
    }

    let limit = max_bytes - 3;
    let mut index = limit;
    while index > 0 && !s.is_char_boundary(index) {
        index -= 1;
    }

    &s[..index]
}

/// Safely truncates an owned string to a max byte-length, appending ellipses (`...`).
pub fn truncate_owned(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let truncated = truncate(s, max_bytes);
    if truncated.is_empty() {
        ".".repeat(max_bytes)
    } else {
        format!("{truncated}...")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello world", 8), "hello");
        assert_eq!(truncate_owned("hello world", 8), "hello...");
        assert_eq!(truncate_owned("hello", 10), "hello");
        assert_eq!(truncate_owned("🦀🦀🦀🦀", 8), "🦀...");
    }
}
