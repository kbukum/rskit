//! String case conversion utilities.

/// Convert a `SCREAMING_SNAKE_CASE` or `snake_case` string to `kebab-case`.
pub fn to_kebab_case(s: &str) -> String {
    s.to_ascii_lowercase().replace('_', "-")
}

/// Convert a `SCREAMING_SNAKE_CASE` or `snake_case` string to `Title Case`.
pub fn to_title_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    let rest: String = chars.map(|c| c.to_ascii_lowercase()).collect();
                    format!("{}{}", first.to_ascii_uppercase(), rest)
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_kebab_case() {
        assert_eq!(to_kebab_case("NOT_FOUND"), "not-found");
        assert_eq!(to_kebab_case("SERVICE_UNAVAILABLE"), "service-unavailable");
        assert_eq!(to_kebab_case("internal_error"), "internal-error");
    }

    #[test]
    fn test_to_title_case() {
        assert_eq!(to_title_case("NOT_FOUND"), "Not Found");
        assert_eq!(to_title_case("SERVICE_UNAVAILABLE"), "Service Unavailable");
        assert_eq!(to_title_case("internal_error"), "Internal Error");
    }
}
