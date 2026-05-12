//! Sensitive data masking engine for structured log output.
//!
//! Provides a [`Masker`] trait (adapter pattern) and a [`DefaultMasker`]
//! implementation that redacts common secrets, PII, and credentials from
//! log output before it reaches any sink.
//!
//! Masking is **on by default** — callers must explicitly disable it via
//! [`MaskingConfig::enabled`].
//!
//! # Examples
//!
//! ```rust
//! use rskit_observability::masking::{DefaultMasker, Masker, MaskingConfig};
//!
//! let masker = DefaultMasker::default();
//! assert_eq!(masker.mask_value("password", "hunter2"), "[REDACTED]");
//! assert_eq!(masker.mask_value("name", "Alice"), "Alice");
//! ```

use std::borrow::Cow;
use std::collections::HashSet;
use std::io;
use std::sync::Arc;

use regex::Regex;
use serde::Deserialize;
use tracing_subscriber::fmt::MakeWriter;

// ── Masker trait ─────────────────────────────────────────────────────────────

/// Trait for masking sensitive values in log output.
///
/// Implement this trait to provide custom masking rules.
pub trait Masker: Send + Sync {
    /// Mask a value based on its field key and content.
    ///
    /// Returns the original value unchanged if no masking is needed,
    /// or a masked version if the value contains sensitive data.
    fn mask_value<'v>(&self, key: &str, value: &'v str) -> Cow<'v, str>;

    /// Mask sensitive data in a formatted log output string.
    ///
    /// Applies both field-name and value-pattern masking to a complete
    /// log line.  Used by [`MaskingMakeWriter`] to mask output before
    /// writing to the underlying writer.
    fn mask_output<'v>(&self, line: &'v str) -> Cow<'v, str>;
}

// ── MaskingConfig ────────────────────────────────────────────────────────────

/// Configuration for sensitive data masking.
#[derive(Debug, Clone, Deserialize)]
pub struct MaskingConfig {
    /// Whether masking is enabled.  Default: `true`.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Additional field names to mask (beyond defaults).
    #[serde(default)]
    pub field_names: Vec<String>,

    /// Additional regex patterns for value masking.
    #[serde(default)]
    pub value_patterns: Vec<String>,

    /// Replacement string for field-name masking.  Default: `"[REDACTED]"`.
    #[serde(default = "default_replacement")]
    pub replacement: String,
}

fn default_true() -> bool {
    true
}

fn default_replacement() -> String {
    "[REDACTED]".to_string()
}

impl Default for MaskingConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            field_names: Vec::new(),
            value_patterns: Vec::new(),
            replacement: default_replacement(),
        }
    }
}

// ── Built-in patterns ────────────────────────────────────────────────────────

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
/// Each tuple is `(regex_source, kind_tag)`.  Ordering matters —
/// bearer-token is checked before JWT so `Bearer <jwt>` is masked as a
/// single unit.
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

// ── DefaultMasker ────────────────────────────────────────────────────────────

/// Production-ready masker with common patterns for PII and secrets.
///
/// Masks by field name (e.g., `password`, `token`) and by value patterns
/// (e.g., email addresses, credit card numbers, JWTs).
///
/// Thread-safe (`Send + Sync`) — create once, share via [`Arc`].
///
/// # Examples
///
/// ```rust
/// use rskit_observability::masking::{DefaultMasker, Masker};
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
    /// Compiles all default and custom regex patterns.  Invalid custom
    /// patterns are silently skipped.
    pub fn new(cfg: &MaskingConfig) -> Self {
        let mut field_names: HashSet<String> = DEFAULT_FIELD_NAMES
            .iter()
            .map(|s| (*s).to_lowercase())
            .collect();
        for name in &cfg.field_names {
            field_names.insert(name.to_lowercase());
        }

        let value_patterns = Self::default_value_patterns();

        let extra_patterns: Vec<Regex> = cfg
            .value_patterns
            .iter()
            .filter_map(|p| Regex::new(p).ok())
            .collect();

        let (json_field_regex, text_field_regex) = Self::build_field_regexes(&field_names);

        Self {
            enabled: cfg.enabled,
            field_names,
            value_patterns,
            extra_patterns,
            json_field_regex,
            text_field_regex,
            replacement: cfg.replacement.clone(),
        }
    }

    /// Compile the hardcoded default value patterns (skipping any that
    /// somehow fail to compile — verified by unit tests).
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
        Self::new(&MaskingConfig::default())
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

// ── Standalone convenience function ──────────────────────────────────────────

/// Convenience: mask a single value using a default masker.
///
/// Useful for one-off masking outside the tracing pipeline.
pub fn mask_value(key: &str, value: &str) -> String {
    use std::sync::LazyLock;
    static DEFAULT: LazyLock<DefaultMasker> = LazyLock::new(DefaultMasker::default);
    DEFAULT.mask_value(key, value).into_owned()
}

// ── MaskingMakeWriter ────────────────────────────────────────────────────────

/// A [`MakeWriter`] wrapper that masks sensitive data in log output.
///
/// Wraps an inner writer and applies masking via the supplied [`Masker`]
/// to every log line before it reaches the underlying output.
///
/// # Examples
///
/// ```ignore
/// use rskit_observability::masking::{DefaultMasker, MaskingMakeWriter, Masker};
/// use std::sync::Arc;
///
/// let masker: Arc<dyn Masker> = Arc::new(DefaultMasker::default());
/// let writer = MaskingMakeWriter::new(std::io::stdout, masker);
/// ```
pub struct MaskingMakeWriter<W> {
    inner: W,
    masker: Arc<dyn Masker>,
}

impl<W> MaskingMakeWriter<W> {
    /// Create a new masking writer wrapper.
    ///
    /// `inner` is the underlying [`MakeWriter`] (e.g., `std::io::stdout`).
    /// `masker` is the masking engine wrapped in an [`Arc`].
    pub fn new(inner: W, masker: Arc<dyn Masker>) -> Self {
        Self { inner, masker }
    }
}

impl<'a, W: MakeWriter<'a>> MakeWriter<'a> for MaskingMakeWriter<W> {
    type Writer = MaskingWriter<W::Writer>;

    fn make_writer(&'a self) -> Self::Writer {
        MaskingWriter {
            inner: self.inner.make_writer(),
            masker: Arc::clone(&self.masker),
            buffer: Vec::with_capacity(256),
        }
    }

    fn make_writer_for(&'a self, meta: &tracing::Metadata<'_>) -> Self::Writer {
        MaskingWriter {
            inner: self.inner.make_writer_for(meta),
            masker: Arc::clone(&self.masker),
            buffer: Vec::with_capacity(256),
        }
    }
}

/// A writer that buffers output and applies masking on flush / drop.
///
/// Created by [`MaskingMakeWriter`].  Buffers all `write` calls and
/// applies masking when the writer is flushed or dropped (at the end of
/// each log event).
pub struct MaskingWriter<W: io::Write> {
    inner: W,
    masker: Arc<dyn Masker>,
    buffer: Vec<u8>,
}

impl<W: io::Write> io::Write for MaskingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if !self.buffer.is_empty() {
            let output = String::from_utf8_lossy(&self.buffer);
            let masked = self.masker.mask_output(&output);
            self.inner.write_all(masked.as_bytes())?;
            self.buffer.clear();
        }
        self.inner.flush()
    }
}

impl<W: io::Write> Drop for MaskingWriter<W> {
    fn drop(&mut self) {
        if !self.buffer.is_empty() {
            // Best-effort flush; errors are silently ignored as is standard
            // practice for log writers.
            let output = String::from_utf8_lossy(&self.buffer);
            let masked = self.masker.mask_output(&output);
            let _ = self.inner.write_all(masked.as_bytes());
            self.buffer.clear();
            let _ = self.inner.flush();
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn default_masker() -> DefaultMasker {
        DefaultMasker::default()
    }

    // ── Default patterns compile ────────────────────────────────────

    #[test]
    fn all_default_value_patterns_compile() {
        let patterns = DefaultMasker::default_value_patterns();
        assert_eq!(
            patterns.len(),
            DEFAULT_VALUE_PATTERNS.len(),
            "some default value patterns failed to compile"
        );
    }

    // ── Field-name masking ──────────────────────────────────────────

    #[test]
    fn masks_password_field() {
        let m = default_masker();
        assert_eq!(m.mask_value("password", "hunter2").as_ref(), "[REDACTED]");
    }

    #[test]
    fn masks_api_key_variants() {
        let m = default_masker();
        assert_eq!(m.mask_value("api_key", "sk-1234").as_ref(), "[REDACTED]");
        assert_eq!(m.mask_value("apikey", "sk-1234").as_ref(), "[REDACTED]");
        assert_eq!(m.mask_value("api-key", "sk-1234").as_ref(), "[REDACTED]");
    }

    #[test]
    fn masks_token_fields() {
        let m = default_masker();
        for field in ["token", "access_token", "refresh_token", "auth_token"] {
            assert_eq!(
                m.mask_value(field, "abc123").as_ref(),
                "[REDACTED]",
                "field: {}",
                field
            );
        }
    }

    #[test]
    fn masks_authorization_field() {
        let m = default_masker();
        assert_eq!(
            m.mask_value("authorization", "Bearer xyz").as_ref(),
            "[REDACTED]"
        );
    }

    #[test]
    fn masks_private_key_field() {
        let m = default_masker();
        assert_eq!(
            m.mask_value("private_key", "-----BEGIN RSA").as_ref(),
            "[REDACTED]"
        );
    }

    #[test]
    fn masks_ssn_field() {
        let m = default_masker();
        assert_eq!(m.mask_value("ssn", "123-45-6789").as_ref(), "[REDACTED]");
    }

    #[test]
    fn masks_credit_card_field() {
        let m = default_masker();
        assert_eq!(
            m.mask_value("credit_card", "4111111111111111").as_ref(),
            "[REDACTED]"
        );
    }

    #[test]
    fn masks_card_number_cvv_pin_secret_fields() {
        let m = default_masker();
        assert_eq!(m.mask_value("card_number", "x").as_ref(), "[REDACTED]");
        assert_eq!(m.mask_value("cvv", "123").as_ref(), "[REDACTED]");
        assert_eq!(m.mask_value("pin", "9876").as_ref(), "[REDACTED]");
        assert_eq!(m.mask_value("secret", "x").as_ref(), "[REDACTED]");
    }

    #[test]
    fn does_not_mask_safe_field() {
        let m = default_masker();
        assert_eq!(m.mask_value("name", "Alice").as_ref(), "Alice");
        assert_eq!(m.mask_value("status", "ok").as_ref(), "ok");
        assert_eq!(m.mask_value("count", "42").as_ref(), "42");
    }

    // ── Case-insensitivity ──────────────────────────────────────────

    #[test]
    fn field_name_masking_is_case_insensitive() {
        let m = default_masker();
        assert_eq!(m.mask_value("Password", "x").as_ref(), "[REDACTED]");
        assert_eq!(m.mask_value("PASSWORD", "x").as_ref(), "[REDACTED]");
        assert_eq!(m.mask_value("pAsSwOrD", "x").as_ref(), "[REDACTED]");
        assert_eq!(m.mask_value("TOKEN", "x").as_ref(), "[REDACTED]");
        assert_eq!(m.mask_value("Api_Key", "x").as_ref(), "[REDACTED]");
    }

    // ── Value-pattern masking ───────────────────────────────────────

    #[test]
    fn masks_email_in_value() {
        let m = default_masker();
        let result = m.mask_value("msg", "contact user@example.com today");
        assert_eq!(result.as_ref(), "contact ***@***.*** today");
    }

    #[test]
    fn masks_credit_card_with_dashes() {
        let m = default_masker();
        let result = m.mask_value("info", "card 4111-1111-1111-1234 used");
        assert_eq!(result.as_ref(), "card ****-****-****-1234 used");
    }

    #[test]
    fn masks_credit_card_no_separators() {
        let m = default_masker();
        let result = m.mask_value("info", "card 4111111111111234 used");
        assert_eq!(result.as_ref(), "card ****-****-****-1234 used");
    }

    #[test]
    fn masks_credit_card_with_spaces() {
        let m = default_masker();
        let result = m.mask_value("data", "4111 1111 1111 9999");
        assert_eq!(result.as_ref(), "****-****-****-9999");
    }

    #[test]
    fn credit_card_preserves_last_four_digits() {
        let m = default_masker();
        let result = m.mask_value("data", "card 1234-5678-9012-3456 end");
        assert!(
            result.contains("3456"),
            "last four digits missing: {}",
            result
        );
        assert!(
            result.contains("****-****-****-"),
            "mask prefix missing: {}",
            result
        );
    }

    #[test]
    fn masks_ssn_in_value() {
        let m = default_masker();
        let result = m.mask_value("data", "ssn is 123-45-6789");
        assert_eq!(result.as_ref(), "ssn is ***-**-****");
    }

    #[test]
    fn masks_ssn_without_dashes() {
        let m = default_masker();
        let result = m.mask_value("data", "ssn is 123456789");
        assert_eq!(result.as_ref(), "ssn is ***-**-****");
    }

    #[test]
    fn masks_jwt_in_value() {
        let m = default_masker();
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.abc123def456";
        let input = format!("token is {}", jwt);
        let result = m.mask_value("data", &input);
        assert_eq!(result.as_ref(), "token is [JWT_REDACTED]");
    }

    #[test]
    fn masks_bearer_token_in_value() {
        let m = default_masker();
        let result = m.mask_value("header", "Bearer abc123def456.xyz");
        assert_eq!(result.as_ref(), "Bearer [REDACTED]");
    }

    #[test]
    fn masks_aws_access_key_in_value() {
        let m = default_masker();
        let result = m.mask_value("data", "key is AKIAIOSFODNN7EXAMPLE");
        assert_eq!(result.as_ref(), "key is [AWS_KEY_REDACTED]");
    }

    #[test]
    fn masks_hex_secret_in_value() {
        let m = default_masker();
        let hex = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4"; // 32 hex chars
        let input = format!("secret: {}", hex);
        let result = m.mask_value("data", &input);
        assert_eq!(result.as_ref(), "secret: [HEX_REDACTED]");
    }

    // ── Custom config ───────────────────────────────────────────────

    #[test]
    fn custom_field_names_are_masked() {
        let cfg = MaskingConfig {
            field_names: vec!["custom_secret".to_string()],
            ..Default::default()
        };
        let m = DefaultMasker::new(&cfg);
        assert_eq!(
            m.mask_value("custom_secret", "hidden").as_ref(),
            "[REDACTED]"
        );
    }

    #[test]
    fn custom_replacement_string() {
        let cfg = MaskingConfig {
            replacement: "***".to_string(),
            ..Default::default()
        };
        let m = DefaultMasker::new(&cfg);
        assert_eq!(m.mask_value("password", "secret").as_ref(), "***");
    }

    #[test]
    fn custom_value_patterns_applied() {
        let cfg = MaskingConfig {
            value_patterns: vec![r"secret_\w+".to_string()],
            ..Default::default()
        };
        let m = DefaultMasker::new(&cfg);
        let result = m.mask_value("msg", "found secret_abc123 in log");
        assert_eq!(result.as_ref(), "found [REDACTED] in log");
    }

    #[test]
    fn invalid_custom_pattern_is_skipped() {
        let cfg = MaskingConfig {
            value_patterns: vec!["[invalid".to_string()],
            ..Default::default()
        };
        // Should not panic; the invalid pattern is silently ignored.
        let m = DefaultMasker::new(&cfg);
        // Default patterns still work.
        let result = m.mask_value("msg", "user@example.com");
        assert!(result.contains("***@***.***"));
    }

    // ── Disabled masking ────────────────────────────────────────────

    #[test]
    fn disabled_masker_passes_through() {
        let cfg = MaskingConfig {
            enabled: false,
            ..Default::default()
        };
        let m = DefaultMasker::new(&cfg);
        assert_eq!(m.mask_value("password", "hunter2").as_ref(), "hunter2");
    }

    #[test]
    fn disabled_mask_output_passes_through() {
        let cfg = MaskingConfig {
            enabled: false,
            ..Default::default()
        };
        let m = DefaultMasker::new(&cfg);
        let line = "{\"password\":\"secret\"}";
        assert_eq!(m.mask_output(line).as_ref(), line);
    }

    // ── Default config ──────────────────────────────────────────────

    #[test]
    fn default_config_is_enabled() {
        let cfg = MaskingConfig::default();
        assert!(cfg.enabled);
        assert_eq!(cfg.replacement, "[REDACTED]");
        assert!(cfg.field_names.is_empty());
        assert!(cfg.value_patterns.is_empty());
    }

    #[test]
    fn default_masker_masks_all_default_fields() {
        let m = default_masker();
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

    // ── mask_output (line-level) ────────────────────────────────────

    #[test]
    fn mask_output_masks_json_field_values() {
        let m = default_masker();
        let input = "{\"password\":\"secret123\",\"name\":\"Alice\"}";
        let output = m.mask_output(input);
        assert!(
            output.contains("\"password\":\"[REDACTED]\""),
            "output: {}",
            output
        );
        assert!(output.contains("\"name\":\"Alice\""), "output: {}", output);
    }

    #[test]
    fn mask_output_masks_text_field_values() {
        let m = default_masker();
        let input = "password=secret123 name=Alice";
        let output = m.mask_output(input);
        assert!(output.contains("password=[REDACTED]"), "output: {}", output);
        assert!(output.contains("name=Alice"), "output: {}", output);
    }

    #[test]
    fn mask_output_masks_email_in_line() {
        let m = default_masker();
        let output = m.mask_output("user email: user@example.com logged in");
        assert!(output.contains("***@***.***"), "output: {}", output);
    }

    #[test]
    fn mask_output_returns_borrowed_when_nothing_to_mask() {
        let m = default_masker();
        let result = m.mask_output("nothing sensitive here");
        assert!(matches!(result, Cow::Borrowed(_)));
    }

    // ── Standalone function ─────────────────────────────────────────

    #[test]
    fn standalone_mask_value_works() {
        assert_eq!(mask_value("password", "hunter2"), "[REDACTED]");
        assert_eq!(mask_value("status", "ok"), "ok");
    }

    // ── Thread safety ───────────────────────────────────────────────

    #[test]
    fn masker_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<DefaultMasker>();
    }

    #[test]
    fn masker_works_across_threads() {
        let masker: Arc<DefaultMasker> = Arc::new(DefaultMasker::default());
        let handles: Vec<_> = (0..4)
            .map(|i| {
                let m = Arc::clone(&masker);
                std::thread::spawn(move || {
                    let input = format!("secret-{}", i);
                    let result = m.mask_value("password", &input);
                    assert_eq!(result.as_ref(), "[REDACTED]");
                })
            })
            .collect();
        for h in handles {
            h.join().expect("thread panicked");
        }
    }

    // ── Serde deserialization ───────────────────────────────────────

    #[test]
    fn config_deserializes_from_json() {
        let json = "{\"enabled\":true,\"field_names\":[\"custom\"],\"replacement\":\"***\"}";
        let cfg: MaskingConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.enabled);
        assert_eq!(cfg.field_names, vec!["custom"]);
        assert_eq!(cfg.replacement, "***");
    }

    #[test]
    fn config_deserializes_with_defaults() {
        let json = "{}";
        let cfg: MaskingConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.enabled);
        assert!(cfg.field_names.is_empty());
        assert_eq!(cfg.replacement, "[REDACTED]");
    }

    // ── Zero-copy ───────────────────────────────────────────────────

    #[test]
    fn returns_borrowed_for_unmasked_value() {
        let m = default_masker();
        let result = m.mask_value("name", "Alice");
        assert!(matches!(result, Cow::Borrowed(_)));
    }

    #[test]
    fn returns_owned_for_masked_value() {
        let m = default_masker();
        let result = m.mask_value("password", "secret");
        assert!(matches!(result, Cow::Owned(_)));
    }
}
