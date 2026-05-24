//! Explicit LLM provider registry.

use std::collections::BTreeMap;
use std::sync::Arc;

use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::Provider;

/// Factory function used to build a configured LLM provider.
pub type Factory = Arc<dyn Fn() -> AppResult<Arc<dyn Provider>> + Send + Sync>;

/// Explicit registry of configured LLM provider factories.
#[derive(Default)]
pub struct Registry {
    factories: BTreeMap<String, Factory>,
}

impl Registry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a configured provider factory under a stable kind.
    pub fn register(&mut self, kind: impl Into<String>, factory: Factory) -> AppResult<()> {
        let kind = Self::normalize_kind(kind)?;
        if self.factories.contains_key(&kind) {
            return Err(AppError::new(
                ErrorCode::AlreadyExists,
                format!("LLM provider '{kind}' is already registered"),
            ));
        }
        self.factories.insert(kind, factory);
        Ok(())
    }

    /// Build the configured provider for `kind`.
    pub fn build(&self, kind: &str) -> AppResult<Arc<dyn Provider>> {
        let kind = Self::normalize_kind(kind)?;
        self.factories.get(&kind).ok_or_else(|| {
            AppError::new(
                ErrorCode::NotFound,
                format!("LLM provider '{kind}' is not registered"),
            )
        })?()
    }

    fn normalize_kind(kind: impl Into<String>) -> AppResult<String> {
        let kind = kind.into().trim().to_owned();
        if kind.is_empty() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "LLM provider kind is required",
            ));
        }
        Ok(kind)
    }

    /// Return registered provider kinds in stable order.
    #[must_use]
    pub fn kinds(&self) -> Vec<&str> {
        self.factories.keys().map(String::as_str).collect()
    }
}

/// Create an empty LLM provider registry.
#[must_use]
pub fn default_registry() -> Registry {
    Registry::new()
}

#[cfg(test)]
mod tests {
    use rskit_errors::ErrorCode;

    use super::Registry;

    #[test]
    fn build_rejects_empty_provider_kind() {
        let Err(err) = Registry::new().build(" \t ") else {
            panic!("empty provider kind should be rejected");
        };
        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }
}
