use std::{collections::BTreeMap, sync::Arc};

use thiserror::Error;

use crate::{Inference, InferenceError};

/// Factory function used to build an inference adapter from config.
pub type Factory =
    Arc<dyn Fn(serde_json::Value) -> Result<Arc<dyn Inference>, InferenceError> + Send + Sync>;

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
        let normalized = kind.trim();
        if normalized.is_empty() {
            return Err(RegistryError::EmptyKind);
        }
        if self.factories.contains_key(normalized) {
            return Err(RegistryError::DuplicateKind(normalized.to_owned()));
        }
        self.factories.insert(normalized.to_owned(), factory);
        Ok(())
    }

    /// Build an adapter from a registered kind and config value.
    pub fn build(
        &self,
        kind: &str,
        config: serde_json::Value,
    ) -> Result<Arc<dyn Inference>, InferenceError> {
        let normalized = kind.trim();
        let factory = self.factories.get(normalized).ok_or_else(|| {
            InferenceError::Decode(format!("unknown inference adapter {normalized:?}"))
        })?;
        factory(config)
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
/// adapter crate `register(&mut Registry)` functions during composition.
#[must_use]
pub fn default_registry() -> Registry {
    Registry::new()
}
