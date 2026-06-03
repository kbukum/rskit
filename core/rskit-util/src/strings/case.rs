/// Convert a string to `snake_case`.
///
/// # Examples
///
/// ```
/// use rskit_util::strings::to_snake_case;
/// assert_eq!(to_snake_case("camelCaseString"), "camel_case_string");
/// assert_eq!(to_snake_case("Kebab-Case-String"), "kebab_case_string");
/// assert_eq!(to_snake_case("already_snake"), "already_snake");
/// ```
pub fn to_snake_case(s: &str) -> String {
    let mut snake = String::with_capacity(s.len() + 4);
    let mut chars = s.chars().peekable();
    let mut is_first = true;

    while let Some(c) = chars.next() {
        if c == '_' || c == '-' || c == ' ' {
            if !is_first && chars.peek().is_some() && !snake.ends_with('_') {
                snake.push('_');
            }
            continue;
        }

        if c.is_uppercase() {
            if !is_first {
                // Peek to see if previous char in snake is already an underscore to avoid duplicates
                if !snake.ends_with('_') {
                    snake.push('_');
                }
            }
            for lc in c.to_lowercase() {
                snake.push(lc);
            }
        } else {
            snake.push(c);
        }
        is_first = false;
    }
    snake
}

/// Convert a string to `kebab-case`.
///
/// # Examples
///
/// ```
/// use rskit_util::strings::to_kebab_case;
/// assert_eq!(to_kebab_case("camelCaseString"), "camel-case-string");
/// assert_eq!(to_kebab_case("already-kebab"), "already-kebab");
/// ```
pub fn to_kebab_case(s: &str) -> String {
    let mut kebab = String::with_capacity(s.len() + 4);
    let mut chars = s.chars().peekable();
    let mut is_first = true;

    while let Some(c) = chars.next() {
        if c == '_' || c == '-' || c == ' ' {
            if !is_first && chars.peek().is_some() && !kebab.ends_with('-') {
                kebab.push('-');
            }
            continue;
        }

        if c.is_uppercase() {
            if !is_first && !kebab.ends_with('-') {
                kebab.push('-');
            }
            for lc in c.to_lowercase() {
                kebab.push(lc);
            }
        } else {
            kebab.push(c);
        }
        is_first = false;
    }
    kebab
}

/// Convert a string to `camelCase`.
///
/// # Examples
///
/// ```
/// use rskit_util::strings::to_camel_case;
/// assert_eq!(to_camel_case("snake_case_string"), "snakeCaseString");
/// assert_eq!(to_camel_case("Kebab-Case-String"), "kebabCaseString");
/// ```
pub fn to_camel_case(s: &str) -> String {
    let mut camel = String::with_capacity(s.len());
    let mut capitalize_next = false;
    let mut is_first = true;

    for c in s.chars() {
        if c == '_' || c == '-' || c == ' ' {
            capitalize_next = true;
            continue;
        }

        if is_first {
            for lc in c.to_lowercase() {
                camel.push(lc);
            }
            is_first = false;
        } else if capitalize_next {
            for uc in c.to_uppercase() {
                camel.push(uc);
            }
            capitalize_next = false;
        } else {
            camel.push(c);
        }
    }
    camel
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("camelCaseString"), "camel_case_string");
        assert_eq!(to_snake_case("Kebab-Case-String"), "kebab_case_string");
        assert_eq!(to_snake_case("already_snake"), "already_snake");
        assert_eq!(to_snake_case("Some  Spaces"), "some_spaces");
    }

    #[test]
    fn test_to_kebab_case() {
        assert_eq!(to_kebab_case("camelCaseString"), "camel-case-string");
        assert_eq!(to_kebab_case("already-kebab"), "already-kebab");
        assert_eq!(to_kebab_case("snake_case_here"), "snake-case-here");
    }

    #[test]
    fn test_to_camel_case() {
        assert_eq!(to_camel_case("snake_case_string"), "snakeCaseString");
        assert_eq!(to_camel_case("Kebab-Case-String"), "kebabCaseString");
        assert_eq!(to_camel_case("AlreadyCamel"), "alreadyCamel");
    }
}
