//! Explicit vector store backend registry.

use std::collections::BTreeMap;
use std::sync::Arc;

use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::{InMemoryVectorStore, VectorStore};

/// Factory for a named vector store backend.
pub trait VectorFactory: Send + Sync {
    /// Create a vector store backend instance.
    fn create(&self) -> AppResult<Arc<dyn VectorStore>>;
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
        let name = name.into();
        if self.factories.insert(name.clone(), factory).is_some() {
            return Err(AppError::new(
                ErrorCode::AlreadyExists,
                format!("vectorstore backend '{name}' is already registered"),
            ));
        }
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

    /// Create a backend by name.
    pub fn build(&self, name: &str) -> AppResult<Arc<dyn VectorStore>> {
        self.factories
            .get(name)
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::NotFound,
                    format!("vectorstore backend '{name}' is not registered"),
                )
            })?
            .create()
    }
}

struct MemoryFactory;

impl VectorFactory for MemoryFactory {
    fn create(&self) -> AppResult<Arc<dyn VectorStore>> {
        Ok(Arc::new(InMemoryVectorStore::new()))
    }
}

/// Explicitly register the in-memory vector store backend.
pub fn register_memory(registry: &mut VectorStoreRegistry) -> AppResult<()> {
    registry.register("memory", Arc::new(MemoryFactory))
}
