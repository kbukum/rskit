//! Explicit messaging backend registry.

use std::collections::HashMap;
use std::sync::Arc;

use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::traits::{MessageConsumer, MessageProducer};

type ProducerFactory<T> = Box<dyn Fn() -> AppResult<Arc<dyn MessageProducer<T>>> + Send + Sync>;
type ConsumerFactory<T> = Box<dyn Fn() -> AppResult<Arc<dyn MessageConsumer<T>>> + Send + Sync>;

/// Application-owned registry of messaging backend factories.
///
/// Backends are registered explicitly by application composition code. Importing
/// a backend module does not mutate global state or dial external services.
pub struct MessagingRegistry<T: Send + Sync + 'static> {
    producers: HashMap<String, ProducerFactory<T>>,
    consumers: HashMap<String, ConsumerFactory<T>>,
}

impl<T: Send + Sync + 'static> MessagingRegistry<T> {
    /// Create an empty messaging registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            producers: HashMap::new(),
            consumers: HashMap::new(),
        }
    }

    /// Register a producer factory under `backend`.
    pub fn register_producer(
        &mut self,
        backend: impl Into<String>,
        factory: impl Fn() -> AppResult<Arc<dyn MessageProducer<T>>> + Send + Sync + 'static,
    ) -> AppResult<()> {
        let backend = backend.into();
        if backend.is_empty() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "messaging backend name is required",
            ));
        }
        if self.producers.contains_key(&backend) {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!("messaging producer backend '{backend}' is already registered"),
            ));
        }
        self.producers.insert(backend, Box::new(factory));
        Ok(())
    }

    /// Register a consumer factory under `backend`.
    pub fn register_consumer(
        &mut self,
        backend: impl Into<String>,
        factory: impl Fn() -> AppResult<Arc<dyn MessageConsumer<T>>> + Send + Sync + 'static,
    ) -> AppResult<()> {
        let backend = backend.into();
        if backend.is_empty() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "messaging backend name is required",
            ));
        }
        if self.consumers.contains_key(&backend) {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!("messaging consumer backend '{backend}' is already registered"),
            ));
        }
        self.consumers.insert(backend, Box::new(factory));
        Ok(())
    }

    /// Construct a producer for `backend`.
    pub fn producer(&self, backend: &str) -> AppResult<Arc<dyn MessageProducer<T>>> {
        let factory = self.producers.get(backend).ok_or_else(|| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("messaging producer backend '{backend}' is not registered"),
            )
        })?;
        factory()
    }

    /// Construct a consumer for `backend`.
    pub fn consumer(&self, backend: &str) -> AppResult<Arc<dyn MessageConsumer<T>>> {
        let factory = self.consumers.get(backend).ok_or_else(|| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("messaging consumer backend '{backend}' is not registered"),
            )
        })?;
        factory()
    }

    /// Return registered producer backend names.
    #[must_use]
    pub fn producer_backends(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.producers.keys().map(String::as_str).collect();
        names.sort_unstable();
        names
    }

    /// Return registered consumer backend names.
    #[must_use]
    pub fn consumer_backends(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.consumers.keys().map(String::as_str).collect();
        names.sort_unstable();
        names
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
        assert!(registry.producer_backends().is_empty());
        assert!(registry.consumer_backends().is_empty());
        assert!(registry.producer("kafka").is_err());
    }

    #[test]
    fn rejects_duplicate_producer_backend() {
        let mut registry = MessagingRegistry::<String>::new();
        let broker = InMemoryBroker::new(8);
        let producer = broker.producer();
        registry
            .register_producer("memory", move || Ok(Arc::new(producer.clone())))
            .unwrap();
        let broker = InMemoryBroker::new(8);
        let producer = broker.producer();
        assert!(
            registry
                .register_producer("memory", move || Ok(Arc::new(producer.clone())))
                .is_err()
        );
    }
}
