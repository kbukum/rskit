//! Safe environment variable parsing with defaults.

use std::str::FromStr;

/// Read a string environment variable with a fallback value.
///
/// # Examples
///
/// ```
/// use rskit_util::env::get_or;
/// let val = get_or("NON_EXISTENT_KEY", "fallback");
/// assert_eq!(val, "fallback");
/// ```
pub fn get_or(key: &str, fallback: impl Into<String>) -> String {
    std::env::var(key).unwrap_or_else(|_| fallback.into())
}

/// Read and parse an environment variable into any type implementing `FromStr`.
/// Returns `None` if the variable is not set or if parsing fails.
///
/// # Examples
///
/// ```
/// use rskit_util::env::get_parsed;
/// let port: Option<u16> = get_parsed("PORT");
/// ```
pub fn get_parsed<T>(key: &str) -> Option<T>
where
    T: FromStr,
{
    std::env::var(key)
        .ok()
        .and_then(|val| val.parse::<T>().ok())
}

/// Safely parse boolean variables.
/// Recognizes "true", "1", "yes", "on" as true, and "false", "0", "no", "off" as false.
/// If the variable is unset or unrecognized, returns the fallback value.
///
/// # Examples
///
/// ```
/// use rskit_util::env::get_bool;
/// let is_enabled = get_bool("FEATURE_FLAG", false);
/// ```
pub fn get_bool(key: &str, fallback: bool) -> bool {
    let Ok(val) = std::env::var(key) else {
        return fallback;
    };
    parse_bool_str(&val, fallback)
}

fn parse_bool_str(val: &str, fallback: bool) -> bool {
    match val.trim().to_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => true,
        "false" | "0" | "no" | "off" => false,
        _ => fallback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_or() {
        assert_eq!(
            get_or("RSKIT_UTIL_TEST_NON_EXISTENT_VAR_12345", "foo"),
            "foo"
        );
        // We know CARGO_MANIFEST_DIR is set by Cargo during tests.
        assert!(!get_or("CARGO_MANIFEST_DIR", "").is_empty());
    }

    #[test]
    fn test_get_parsed() {
        assert_eq!(
            get_parsed::<u16>("RSKIT_UTIL_TEST_NON_EXISTENT_VAR_12345"),
            None
        );
        // CARGO_PKG_VERSION_MAJOR is set by Cargo to a valid integer.
        let expected: u16 = std::env::var("CARGO_PKG_VERSION_MAJOR")
            .expect("CARGO_PKG_VERSION_MAJOR set by Cargo")
            .parse()
            .expect("CARGO_PKG_VERSION_MAJOR parses as u16");
        assert_eq!(get_parsed::<u16>("CARGO_PKG_VERSION_MAJOR"), Some(expected));
    }

    #[test]
    fn test_get_bool() {
        assert!(get_bool("RSKIT_UTIL_TEST_NON_EXISTENT_VAR_12345", true));
        assert!(!get_bool("RSKIT_UTIL_TEST_NON_EXISTENT_VAR_12345", false));
    }

    #[test]
    fn test_parse_bool_str() {
        assert!(parse_bool_str("true", false));
        assert!(parse_bool_str("1", false));
        assert!(parse_bool_str("yes", false));
        assert!(parse_bool_str("on", false));

        assert!(!parse_bool_str("false", true));
        assert!(!parse_bool_str("0", true));
        assert!(!parse_bool_str("no", true));
        assert!(!parse_bool_str("off", true));

        assert!(parse_bool_str("maybe", true));
        assert!(!parse_bool_str("maybe", false));
    }
}
