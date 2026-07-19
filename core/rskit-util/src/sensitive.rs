//! Matching helpers for names that commonly carry secret values.

/// Default key names that should usually be treated as secret-bearing.
pub const DEFAULT_SECRET_KEY_NAMES: &[&str] = &[
    "password",
    "passwd",
    "pwd",
    "token",
    "secret",
    "credential",
    "credentials",
    "apikey",
    "api_key",
    "auth",
    "authorization",
    "auth_token",
    "access_token",
    "refresh_token",
    "client_secret",
];

/// Matches normalized key names that commonly carry secret values.
///
/// Names are compared case-insensitively after trimming leading dashes and normalizing `-` to `_`.
/// A key matches when it equals a configured name or ends with `_<name>`,
/// allowing names such as `db_password` while avoiding false positives such as `author`.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SecretKeyMatcher {
    names: Vec<String>,
}

impl Default for SecretKeyMatcher {
    fn default() -> Self {
        Self::new(DEFAULT_SECRET_KEY_NAMES)
    }
}

impl SecretKeyMatcher {
    /// Create a matcher from a set of secret-bearing names.
    #[must_use]
    pub fn new(names: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        let names = names
            .into_iter()
            .filter_map(|name| normalize_name(name.as_ref()))
            .collect();
        Self { names }
    }

    /// Add another secret-bearing key name.
    #[must_use]
    pub fn with_name(mut self, name: impl AsRef<str>) -> Self {
        if let Some(name) = normalize_name(name.as_ref())
            && !self.names.iter().any(|existing| existing == &name)
        {
            self.names.push(name);
        }
        self
    }

    /// Add multiple secret-bearing key names.
    #[must_use]
    pub fn with_names(mut self, names: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        for name in names {
            self = self.with_name(name);
        }
        self
    }

    /// Return true when `name` should be treated as secret-bearing.
    #[must_use]
    pub fn is_secret_key(&self, name: &str) -> bool {
        let Some(normalized) = normalize_name(name) else {
            return false;
        };

        self.names
            .iter()
            .any(|secret| normalized == secret.as_str() || has_secret_suffix(&normalized, secret))
    }
}

fn has_secret_suffix(normalized: &str, secret: &str) -> bool {
    normalized
        .strip_suffix(secret)
        .is_some_and(|prefix| prefix.ends_with('_'))
}

fn normalize_name(name: &str) -> Option<String> {
    let normalized = name
        .trim_start_matches('-')
        .replace('-', "_")
        .to_ascii_lowercase();

    (!normalized.is_empty()).then_some(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_matches_common_secret_key_names() {
        let matcher = SecretKeyMatcher::default();

        assert!(matcher.is_secret_key("--token"));
        assert!(matcher.is_secret_key("--auth-token"));
        assert!(matcher.is_secret_key("db_password"));
        assert!(matcher.is_secret_key("CLIENT_SECRET"));
    }

    #[test]
    fn default_does_not_match_non_secret_substrings() {
        let matcher = SecretKeyMatcher::default();

        assert!(!matcher.is_secret_key("--author"));
        assert!(!matcher.is_secret_key("tokenizer"));
    }

    #[test]
    fn custom_names_extend_matching() {
        let matcher = SecretKeyMatcher::default().with_name("license-key");

        assert!(matcher.is_secret_key("--license-key"));
        assert!(matcher.is_secret_key("vendor_license_key"));
    }
}
