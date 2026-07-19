//! Redaction policy for command-line arguments emitted in spawn logs.

use rskit_util::SecretKeyMatcher;

/// Redaction policy for command-line arguments emitted in process spawn logs.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct ArgRedaction {
    matcher: SecretKeyMatcher,
}

impl ArgRedaction {
    /// Create a policy with the default secret-bearing argument names.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the default secret-bearing names with the provided names.
    #[must_use]
    pub fn from_names(names: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        Self {
            matcher: SecretKeyMatcher::new(names),
        }
    }

    /// Add a secret-bearing argument name.
    #[must_use]
    pub fn with_name(mut self, name: impl AsRef<str>) -> Self {
        self.matcher = self.matcher.with_name(name);
        self
    }

    /// Add multiple secret-bearing argument names.
    #[must_use]
    pub fn with_names(mut self, names: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        self.matcher = self.matcher.with_names(names);
        self
    }

    /// Return true when an argument name should have its value redacted.
    #[must_use]
    pub fn is_sensitive_arg_name(&self, name: &str) -> bool {
        self.matcher.is_secret_key(name)
    }
}
