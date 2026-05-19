//! Explicit messaging adapter registry.

use std::collections::BTreeMap;
use std::sync::Arc;

use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::config::BrokerConfig;
use crate::traits::{MessageConsumer, MessageProducer};

/// Producer/consumer pair created for a messaging backend.
pub struct MessagingBackend<T: Send + Sync + 'static> {
    /// Producer for the selected backend.
    pub producer: Arc<dyn MessageProducer<T>>,
    /// Consumer for the selected backend.
    pub consumer: Arc<dyn MessageConsumer<T>>,
}

/// Factory for a named messaging backend.
pub trait MessagingFactory<T: Send + Sync + 'static>: Send + Sync {
    /// Build a producer/consumer pair from broker configuration.
    fn create(&self, config: &BrokerConfig) -> AppResult<MessagingBackend<T>>;
}

/// Application-owned registry of messaging adapter factories.
///
/// Adapters are registered explicitly by application composition code. Importing
/// an adapter module does not mutate global state or dial external services.
pub struct MessagingRegistry<T: Send + Sync + 'static> {
    factories: BTreeMap<String, Arc<dyn MessagingFactory<T>>>,
}

impl<T: Send + Sync + 'static> MessagingRegistry<T> {
    /// Create an empty messaging registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            factories: BTreeMap::new(),
        }
    }

    /// Register a backend factory under `adapter`.
    pub fn register_backend(
        &mut self,
        adapter: impl Into<String>,
        factory: Arc<dyn MessagingFactory<T>>,
    ) -> AppResult<()> {
        let adapter = adapter.into();
        if adapter.is_empty() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "messaging adapter name is required",
            ));
        }
        if self.factories.contains_key(&adapter) {
            return Err(AppError::new(
                ErrorCode::AlreadyExists,
                format!("messaging adapter '{adapter}' is already registered"),
            ));
        }
        self.factories.insert(adapter, factory);
        Ok(())
    }

    /// Build producer and consumer for [`BrokerConfig::adapter`].
    pub fn build(&self, config: &BrokerConfig) -> AppResult<MessagingBackend<T>> {
        config.validate()?;
        let factory = self.factories.get(&config.adapter).ok_or_else(|| {
            AppError::new(
                ErrorCode::NotFound,
                format!("messaging adapter '{}' is not registered", config.adapter),
            )
        })?;
        factory.create(config)
    }

    /// Construct a producer for [`BrokerConfig::adapter`].
    pub fn producer(&self, config: &BrokerConfig) -> AppResult<Arc<dyn MessageProducer<T>>> {
        self.build(config).map(|backend| backend.producer)
    }

    /// Construct a consumer for [`BrokerConfig::adapter`].
    pub fn consumer(&self, config: &BrokerConfig) -> AppResult<Arc<dyn MessageConsumer<T>>> {
        self.build(config).map(|backend| backend.consumer)
    }

    /// Return registered adapter names.
    #[must_use]
    pub fn adapters(&self) -> Vec<&str> {
        self.factories.keys().map(String::as_str).collect()
    }

    /// Return true when no adapters are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.factories.is_empty()
    }

    /// Number of registered adapters.
    #[must_use]
    pub fn len(&self) -> usize {
        self.factories.len()
    }

    /// Return true when `adapter` is registered.
    #[must_use]
    pub fn contains(&self, adapter: &str) -> bool {
        self.factories.contains_key(adapter)
    }
}

impl<T: Send + Sync + 'static> Default for MessagingRegistry<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::InMemoryBroker;

    #[test]
    fn new_registry_is_empty_and_has_no_side_effects() {
        let registry = MessagingRegistry::<String>::new();
        assert!(registry.is_empty());
        assert!(registry.adapters().is_empty());
        assert!(registry.producer(&BrokerConfig::default()).is_err());
    }

    #[test]
    fn rejects_duplicate_adapter() {
        let mut registry = MessagingRegistry::<String>::new();
        let broker = InMemoryBroker::new(8);
        crate::memory::register(&mut registry, broker.clone()).unwrap();

        let err = crate::memory::register(&mut registry, broker).unwrap_err();

        assert_eq!(err.code, ErrorCode::AlreadyExists);
    }
}
