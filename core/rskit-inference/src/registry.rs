use std::{collections::BTreeMap, sync::Arc};

use thiserror::Error;

use crate::{Inference, InferenceError};

/// Factory function used to build a configured inference adapter.
pub type Factory = Arc<dyn Fn() -> Result<Arc<dyn Inference>, InferenceError> + Send + Sync>;

/// Explicit registry of inference adapter factories.
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

    /// Register an adapter factory under a stable kind.
    pub fn register(&mut self, kind: &str, factory: Factory) -> Result<(), RegistryError> {
        let kind = Self::normalize_kind(kind).map_err(|_| RegistryError::EmptyKind)?;
        if self.factories.contains_key(&kind) {
            return Err(RegistryError::DuplicateKind(kind));
        }
        self.factories.insert(kind, factory);
        Ok(())
    }

    /// Build the configured adapter registered for `kind`.
    pub fn build(&self, kind: &str) -> Result<Arc<dyn Inference>, InferenceError> {
        let kind = Self::normalize_kind(kind)?;
        let factory = self
            .factories
            .get(&kind)
            .ok_or_else(|| InferenceError::Decode(format!("unknown inference adapter {kind:?}")))?;
        factory()
    }

    fn normalize_kind(kind: &str) -> Result<String, InferenceError> {
        let kind = kind.trim();
        if kind.is_empty() {
            return Err(InferenceError::InvalidInput(
                "inference adapter kind is required".to_owned(),
            ));
        }
        Ok(kind.to_owned())
    }

    /// Return registered kinds in stable order.
    #[must_use]
    pub fn kinds(&self) -> Vec<String> {
        self.factories.keys().cloned().collect()
    }
}

/// Registry mutation failure.
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum RegistryError {
    /// Adapter kind was empty or whitespace.
    #[error("inference adapter kind is required")]
    EmptyKind,
    /// Adapter kind is already registered.
    #[error("inference adapter {0:?} already registered")]
    DuplicateKind(String),
}

/// Create an empty registry.
///
/// Backends are intentionally not auto-registered. Consumers opt in by calling
/// adapter crate `register(&mut Registry, Config)` functions during composition.
#[must_use]
pub fn default_registry() -> Registry {
    Registry::new()
}

#[cfg(test)]
mod tests {
    use rskit_errors::ErrorCode;

    use super::{Registry, RegistryError};
    use crate::InferenceError;

    #[test]
    fn register_rejects_empty_kind() {
        let mut registry = Registry::new();
        let factory = std::sync::Arc::new(|| unreachable!("factory should not run"));
        let err = registry.register(" \t ", factory).unwrap_err();
        assert_eq!(err, RegistryError::EmptyKind);
    }

    #[test]
    fn build_rejects_empty_kind_as_invalid_input() {
        let Err(err) = Registry::new().build(" \t ") else {
            panic!("empty inference adapter kind should be rejected");
        };
        assert!(matches!(err, InferenceError::InvalidInput(_)));

        let app_error = rskit_errors::AppError::from(err);
        assert_eq!(app_error.code(), ErrorCode::InvalidInput);
    }
}
