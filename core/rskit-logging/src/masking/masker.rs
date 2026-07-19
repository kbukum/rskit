//! Masker abstraction for redacting sensitive values in log output.

use std::borrow::Cow;

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
    /// Applies both field-name and value-pattern masking to a complete log line.
    /// Used by [`MaskingMakeWriter`](super::MaskingMakeWriter) to mask output before writing to the underlying writer.
    fn mask_output<'v>(&self, line: &'v str) -> Cow<'v, str>;
}
