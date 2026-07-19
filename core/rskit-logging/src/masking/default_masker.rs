//! Default masker implementation with built-in secret and PII patterns.

use std::borrow::Cow;
use std::collections::HashSet;

use regex::Regex;

use super::config::MaskingConfig;
use super::masker::Masker;
use crate::error::{self, LoggingResult};

/// Default sensitive field names (case-insensitive matching).
const DEFAULT_FIELD_NAMES: &[&str] = &[
    "password",
    "secret",
    "token",
    "api_key",
    "apikey",
    "api-key",
    "authorization",
    "auth_token",
    "access_token",
    "refresh_token",
    "private_key",
    "ssn",
    "credit_card",
    "card_number",
    "cvv",
    "pin",
];

/// Internal value-pattern entry.
struct MaskPattern {
    regex: Regex,
    kind: PatternKind,
}

/// Whether to use a static replacement or special credit-card logic.
enum PatternKind {
    /// Replace the entire match with a static string.
    Replace(&'static str),
    /// Replace preserving the last 4 digits.
    CreditCard,
}

/// Hard-coded default value patterns with their replacement kinds.
///
/// Each tuple is `(regex_source, kind_tag)`. Ordering matters — bearer-token is checked before JWT
/// so `Bearer <jwt>` is masked as a single unit.
const DEFAULT_VALUE_PATTERNS: &[(&str, u8)] = &[
    // 0 = Bearer token (case-insensitive)
    (r"(?i)Bearer\s+[a-zA-Z0-9._~+/=-]+", 0),
    // 1 = JWT
    (
        r"eyJ[a-zA-Z0-9_-]{10,}\.eyJ[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]+",
        1,
    ),
    // 2 = AWS Access Key
    (r"AKIA[0-9A-Z]{16}", 2),
    // 3 = Credit Card (16 digits with optional separators)
    (r"\b\d{4}[\s-]?\d{4}[\s-]?\d{4}[\s-]?\d{4}\b", 3),
    // 4 = SSN
    (r"\b\d{3}-?\d{2}-?\d{4}\b", 4),
    // 5 = Email
    (r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}", 5),
    // 6 = Generic hex secret (32+ chars)
    (r"\b[0-9a-fA-F]{32,}\b", 6),
];

/// Map the compact `u8` tag back to a [`PatternKind`].
fn tag_to_kind(tag: u8) -> PatternKind {
    match tag {
        0 => PatternKind::Replace("Bearer [REDACTED]"),
        1 => PatternKind::Replace("[JWT_REDACTED]"),
        2 => PatternKind::Replace("[AWS_KEY_REDACTED]"),
        3 => PatternKind::CreditCard,
        4 => PatternKind::Replace("***-**-****"),
        5 => PatternKind::Replace("***@***.***"),
        6 => PatternKind::Replace("[HEX_REDACTED]"),
        _ => PatternKind::Replace("[REDACTED]"),
    }
}

/// Production-ready masker with common patterns for PII and secrets.
///
/// Masks by field name (e.g., `password`, `token`)
/// and by value patterns (e.g., email addresses, credit card numbers, JWTs).
///
/// Thread-safe (`Send + Sync`) — create once, share via [`Arc`](std::sync::Arc).
///
/// # Examples
///
/// ```rust
/// use rskit_logging::masking::{DefaultMasker, Masker};
///
/// let masker = DefaultMasker::default();
///
/// // Field-name masking
/// let masked = masker.mask_value("password", "my-secret");
/// assert_eq!(masked, "[REDACTED]");
///
/// // Email masking
/// let masked = masker.mask_value("msg", "contact user@example.com");
/// assert!(masked.contains("***@***.***"));
/// ```
pub struct DefaultMasker {
    /// Whether masking is active.
    enabled: bool,
    /// Lower-cased field names for fast lookup.
    field_names: HashSet<String>,
    /// Compiled regex patterns for value masking.
    value_patterns: Vec<MaskPattern>,
    /// User-supplied extra regex patterns.
    extra_patterns: Vec<Regex>,
    /// Pre-compiled regex for JSON field masking: `"field":"value"`.
    json_field_regex: Option<Regex>,
    /// Pre-compiled regex for text field masking: `field=value`.
    text_field_regex: Option<Regex>,
    /// Replacement string for field-name masking.
    replacement: String,
}

impl DefaultMasker {
    /// Create a new masker from the given configuration.
    ///
    /// Compiles all default and custom regex patterns.
    ///
    /// # Errors
    ///
    /// Returns an error when any custom value pattern is not a valid regex.
    pub fn new(cfg: &MaskingConfig) -> LoggingResult<Self> {
        let mut field_names: HashSet<String> = DEFAULT_FIELD_NAMES
            .iter()
            .map(|s| (*s).to_lowercase())
            .collect();
        for name in &cfg.field_names {
            field_names.insert(name.to_lowercase());
        }

        let value_patterns = Self::default_value_patterns();

        let mut extra_patterns = Vec::with_capacity(cfg.value_patterns.len());
        for pattern in &cfg.value_patterns {
            let regex =
                Regex::new(pattern).map_err(|err| error::invalid_regex(pattern.clone(), err))?;
            extra_patterns.push(regex);
        }

        let (json_field_regex, text_field_regex) = Self::build_field_regexes(&field_names);

        Ok(Self {
            enabled: cfg.enabled,
            field_names,
            value_patterns,
            extra_patterns,
            json_field_regex,
            text_field_regex,
            replacement: cfg.replacement.clone(),
        })
    }

    /// Compile the hardcoded default value patterns (skipping any that somehow fail to compile — verified by unit tests).
    fn default_value_patterns() -> Vec<MaskPattern> {
        DEFAULT_VALUE_PATTERNS
            .iter()
            .filter_map(|&(src, tag)| {
                Regex::new(src).ok().map(|regex| MaskPattern {
                    regex,
                    kind: tag_to_kind(tag),
                })
            })
            .collect()
    }

    /// Build field-name regexes for JSON and text log formats.
    fn build_field_regexes(field_names: &HashSet<String>) -> (Option<Regex>, Option<Regex>) {
        if field_names.is_empty() {
            return (None, None);
        }

        let escaped: Vec<String> = field_names.iter().map(|s| regex::escape(s)).collect();
        let alt = escaped.join("|");

        // JSON: "field_name" : "value"
        let json_src = format!("(?i)\"({})\"\\s*:\\s*\"([^\"]*)\"", alt);
        // Text: field_name=value (until whitespace, comma, brace, or quote)
        let text_src = format!("(?i)({})=([^\\s,}}\"]+)", alt);

        (Regex::new(&json_src).ok(), Regex::new(&text_src).ok())
    }

    /// Apply a single built-in value pattern, returning the masked result.
    fn apply_value_pattern<'a>(&self, pattern: &MaskPattern, input: &'a str) -> Cow<'a, str> {
        match pattern.kind {
            PatternKind::Replace(replacement) => pattern.regex.replace_all(input, replacement),
            PatternKind::CreditCard => {
                pattern.regex.replace_all(input, |caps: &regex::Captures| {
                    let matched = &caps[0];
                    let digits: String = matched.chars().filter(|c| c.is_ascii_digit()).collect();
                    if digits.len() >= 4 {
                        let last4 = &digits[digits.len() - 4..];
                        format!("****-****-****-{}", last4)
                    } else {
                        "[CARD_REDACTED]".to_string()
                    }
                })
            }
        }
    }

    /// Apply value-level patterns (built-in + extra) to a string.
    fn apply_all_value_patterns<'v>(&self, value: &'v str) -> Cow<'v, str> {
        let mut result = Cow::Borrowed(value);

        for pattern in &self.value_patterns {
            if pattern.regex.is_match(&result) {
                result = Cow::Owned(self.apply_value_pattern(pattern, &result).into_owned());
            }
        }

        for extra in &self.extra_patterns {
            if extra.is_match(&result) {
                result = Cow::Owned(
                    extra
                        .replace_all(&result, self.replacement.as_str())
                        .into_owned(),
                );
            }
        }

        result
    }
}

impl Default for DefaultMasker {
    fn default() -> Self {
        let cfg = MaskingConfig::default();
        let mut field_names: HashSet<String> = DEFAULT_FIELD_NAMES
            .iter()
            .map(|s| (*s).to_lowercase())
            .collect();
        for name in &cfg.field_names {
            field_names.insert(name.to_lowercase());
        }
        let (json_field_regex, text_field_regex) = Self::build_field_regexes(&field_names);
        Self {
            enabled: cfg.enabled,
            field_names,
            value_patterns: Self::default_value_patterns(),
            extra_patterns: Vec::new(),
            json_field_regex,
            text_field_regex,
            replacement: cfg.replacement,
        }
    }
}

impl Masker for DefaultMasker {
    fn mask_value<'v>(&self, key: &str, value: &'v str) -> Cow<'v, str> {
        if !self.enabled {
            return Cow::Borrowed(value);
        }

        // Fast path: field-name match (case-insensitive).
        if !key.is_empty() && self.field_names.contains(&key.to_lowercase()) {
            return Cow::Owned(self.replacement.clone());
        }

        // Slow path: value-pattern regexes.
        self.apply_all_value_patterns(value)
    }

    fn mask_output<'v>(&self, line: &'v str) -> Cow<'v, str> {
        if !self.enabled {
            return Cow::Borrowed(line);
        }

        let mut result = Cow::Borrowed(line);

        // Value patterns.
        for pattern in &self.value_patterns {
            if pattern.regex.is_match(&result) {
                result = Cow::Owned(self.apply_value_pattern(pattern, &result).into_owned());
            }
        }

        for extra in &self.extra_patterns {
            if extra.is_match(&result) {
                result = Cow::Owned(
                    extra
                        .replace_all(&result, self.replacement.as_str())
                        .into_owned(),
                );
            }
        }

        // JSON field-name masking: "password":"secret" -> "password":"[REDACTED]"
        if let Some(ref re) = self.json_field_regex
            && re.is_match(&result)
        {
            let replacement = &self.replacement;
            let masked = re.replace_all(&result, |caps: &regex::Captures| {
                format!("\"{}\":\"{}\"", &caps[1], replacement)
            });
            result = Cow::Owned(masked.into_owned());
        }

        // Text field-name masking: password=secret -> password=[REDACTED]
        if let Some(ref re) = self.text_field_regex
            && re.is_match(&result)
        {
            let replacement = &self.replacement;
            let masked = re.replace_all(&result, |caps: &regex::Captures| {
                format!("{}={}", &caps[1], replacement)
            });
            result = Cow::Owned(masked.into_owned());
        }

        result
    }
}

/// Convenience: mask a single value using a default masker.
///
/// Useful for one-off masking outside the tracing pipeline.
pub fn mask_value(key: &str, value: &str) -> String {
    use std::sync::LazyLock;
    static DEFAULT: LazyLock<DefaultMasker> = LazyLock::new(DefaultMasker::default);
    DEFAULT.mask_value(key, value).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_default_value_patterns_compile() {
        let patterns = DefaultMasker::default_value_patterns();
        assert_eq!(
            patterns.len(),
            DEFAULT_VALUE_PATTERNS.len(),
            "some default value patterns failed to compile"
        );
    }

    #[test]
    fn default_masker_masks_all_default_fields() {
        let m = DefaultMasker::default();
        for field in DEFAULT_FIELD_NAMES {
            let result = m.mask_value(field, "test-value");
            assert_eq!(
                result.as_ref(),
                "[REDACTED]",
                "field '{}' not masked",
                field
            );
        }
    }
}
