//! Explicit vector store backend registry.

use std::collections::BTreeMap;
use std::sync::Arc;

use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::{InMemoryVectorStore, VectorStore, VectorStoreConfig};

/// Factory for a named vector store backend.
pub trait VectorFactory: Send + Sync {
    /// Create a vector store backend instance.
    fn create(&self, config: &VectorStoreConfig) -> AppResult<Arc<dyn VectorStore>>;
}

/// Explicit vector store backend registry.
#[derive(Default)]
pub struct VectorStoreRegistry {
    factories: BTreeMap<String, Arc<dyn VectorFactory>>,
}

impl VectorStoreRegistry {
    /// Create an empty registry. No backends are registered implicitly.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a backend factory under `name`.
    pub fn register(
        &mut self,
        name: impl Into<String>,
        factory: Arc<dyn VectorFactory>,
    ) -> AppResult<()> {
        let name = name.into().trim().to_owned();
        if name.is_empty() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "vectorstore backend name is required",
            ));
        }
        if self.factories.contains_key(&name) {
            return Err(AppError::new(
                ErrorCode::AlreadyExists,
                format!("vectorstore backend '{name}' is already registered"),
            ));
        }
        self.factories.insert(name, factory);
        Ok(())
    }

    /// Return true when a backend is registered.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.factories.contains_key(name)
    }

    /// Number of registered backends.
    #[must_use]
    pub fn len(&self) -> usize {
        self.factories.len()
    }

    /// Return true when no backend factories are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.factories.is_empty()
    }

    /// Build the backend selected by [`VectorStoreConfig::backend`].
    pub fn build(&self, config: &VectorStoreConfig) -> AppResult<Arc<dyn VectorStore>> {
        let backend = config.backend.trim();
        if backend.is_empty() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "vectorstore backend name is required",
            ));
        }
        self.factories
            .get(backend)
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::NotFound,
                    format!("vectorstore backend '{backend}' is not registered"),
                )
            })?
            .create(config)
    }
}

struct MemoryFactory;

impl VectorFactory for MemoryFactory {
    fn create(&self, config: &VectorStoreConfig) -> AppResult<Arc<dyn VectorStore>> {
        Ok(Arc::new(InMemoryVectorStore::with_metric(
            config.memory.metric,
        )))
    }
}

/// Explicitly register the in-memory vector store backend.
pub fn register_memory(registry: &mut VectorStoreRegistry) -> AppResult<()> {
    registry.register("memory", Arc::new(MemoryFactory))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_starts_empty() {
        let registry = VectorStoreRegistry::new();

        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
        assert!(!registry.contains("memory"));
    }

    #[test]
    fn registry_rejects_duplicate_backend() {
        let mut registry = VectorStoreRegistry::new();
        register_memory(&mut registry).unwrap();

        let err = register_memory(&mut registry).expect_err("duplicate backend must fail");

        assert_eq!(err.code, ErrorCode::AlreadyExists);
    }

    #[test]
    fn registry_rejects_blank_registration_name() {
        let mut registry = VectorStoreRegistry::new();

        let err = registry
            .register(" ", Arc::new(MemoryFactory))
            .err()
            .unwrap();

        assert_eq!(err.code, ErrorCode::InvalidInput);
    }

    #[test]
    fn registry_builds_memory_backend_after_explicit_registration() {
        let mut registry = VectorStoreRegistry::new();
        register_memory(&mut registry).unwrap();

        let store = registry.build(&VectorStoreConfig::default()).unwrap();

        assert!(Arc::strong_count(&store) >= 1);
    }

    #[test]
    fn registry_rejects_unregistered_backend() {
        let registry = VectorStoreRegistry::new();

        let err = registry.build(&VectorStoreConfig::default()).err().unwrap();

        assert_eq!(err.code, ErrorCode::NotFound);
    }

    #[test]
    fn registry_rejects_blank_backend() {
        let registry = VectorStoreRegistry::new();
        let config = VectorStoreConfig {
            backend: "  ".to_owned(),
            ..VectorStoreConfig::default()
        };

        let err = registry.build(&config).err().unwrap();

        assert_eq!(err.code, ErrorCode::InvalidInput);
    }

    #[test]
    fn registry_normalizes_backend_before_lookup() {
        let mut registry = VectorStoreRegistry::new();
        register_memory(&mut registry).unwrap();
        let config = VectorStoreConfig {
            backend: " memory ".to_owned(),
            ..VectorStoreConfig::default()
        };

        let store = registry.build(&config).unwrap();

        assert!(Arc::strong_count(&store) >= 1);
    }
}
