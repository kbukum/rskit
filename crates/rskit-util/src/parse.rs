//! Human-readable size parsing and secret masking.

/// Parse a human-readable size string (e.g. `"10MB"`, `"512KB"`, `"2GB"`)
/// into bytes.  Returns `default_bytes` if the string cannot be parsed.
pub fn parse_size(s: &str, default_bytes: i64) -> i64 {
    let s = s.trim().to_uppercase();
    if s.is_empty() {
        return default_bytes;
    }

    let (suffix_len, multiplier): (usize, i64) = if s.ends_with("TB") {
        (2, 1024 * 1024 * 1024 * 1024)
    } else if s.ends_with("GB") {
        (2, 1024 * 1024 * 1024)
    } else if s.ends_with("MB") {
        (2, 1024 * 1024)
    } else if s.ends_with("KB") {
        (2, 1024)
    } else {
        (0, 1)
    };

    let num_part = &s[..s.len() - suffix_len];
    num_part
        .trim()
        .parse::<i64>()
        .map_or(default_bytes, |v| v * multiplier)
}

/// Mask a secret for safe display in logs.
///
/// If the string is shorter than or equal to `visible_prefix`, it is fully
/// masked as `"***"`.
pub fn mask_secret(s: &str, visible_prefix: usize) -> String {
    if s.len() <= visible_prefix {
        return "***".to_string();
    }
    format!("{}***", &s[..visible_prefix])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_size_known_units() {
        assert_eq!(parse_size("10MB", 0), 10 * 1024 * 1024);
        assert_eq!(parse_size("512KB", 0), 512 * 1024);
        assert_eq!(parse_size("2GB", 0), 2 * 1024 * 1024 * 1024);
        assert_eq!(parse_size("1TB", 0), 1024_i64 * 1024 * 1024 * 1024);
        assert_eq!(parse_size("1024", 0), 1024);
    }

    #[test]
    fn parse_size_case_insensitive_and_trimmed() {
        assert_eq!(parse_size("  10mb  ", 0), 10 * 1024 * 1024);
        assert_eq!(parse_size("10Mb", 0), 10 * 1024 * 1024);
    }

    #[test]
    fn parse_size_returns_default() {
        assert_eq!(parse_size("", 42), 42);
        assert_eq!(parse_size("invalid", 99), 99);
    }

    #[test]
    fn mask_secret_cases() {
        assert_eq!(mask_secret("abcdef", 3), "abc***");
        assert_eq!(mask_secret("short", 10), "***");
        assert_eq!(mask_secret("", 5), "***");
        assert_eq!(
            mask_secret("host=localhost user=admin password=secret", 10),
            "host=local***"
        );
    }
}
