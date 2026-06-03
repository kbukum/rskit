/// Safely truncates a string to a max byte-length, appending ellipses (`...`)
/// without splitting UTF-8 code points.
///
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

    // Since we append "..." which takes 3 bytes, the cut-off point must be max_bytes - 3
    if max_bytes <= 3 {
        return &s[..0]; // too short to even contain the ellipsis
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
