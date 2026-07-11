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
    /// Build a producer from broker configuration.
    fn create_producer(&self, config: &BrokerConfig) -> AppResult<Arc<dyn MessageProducer<T>>>;

    /// Build a consumer from broker configuration.
    fn create_consumer(&self, config: &BrokerConfig) -> AppResult<Arc<dyn MessageConsumer<T>>>;

    /// Build a producer/consumer pair from broker configuration.
    fn create(&self, config: &BrokerConfig) -> AppResult<MessagingBackend<T>> {
        Ok(MessagingBackend {
            producer: self.create_producer(config)?,
            consumer: self.create_consumer(config)?,
        })
    }
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
        let adapter = adapter.into().trim().to_owned();
        BrokerConfig::new(adapter.clone()).validate()?;
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
        config.validate()?;
        let factory = self.factories.get(&config.adapter).ok_or_else(|| {
            AppError::new(
                ErrorCode::NotFound,
                format!("messaging adapter '{}' is not registered", config.adapter),
            )
        })?;
        factory.create_producer(config)
    }

    /// Construct a consumer for [`BrokerConfig::adapter`].
    pub fn consumer(&self, config: &BrokerConfig) -> AppResult<Arc<dyn MessageConsumer<T>>> {
        config.validate()?;
        let factory = self.factories.get(&config.adapter).ok_or_else(|| {
            AppError::new(
                ErrorCode::NotFound,
                format!("messaging adapter '{}' is not registered", config.adapter),
            )
        })?;
        factory.create_consumer(config)
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
    use crate::message::Message;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

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

        assert_eq!(err.code(), ErrorCode::AlreadyExists);
    }

    #[test]
    fn rejects_blank_adapter_registration() {
        let mut registry = MessagingRegistry::<String>::new();
        let factory = CountingFactory {
            producer_calls: Arc::new(AtomicUsize::new(0)),
            consumer_calls: Arc::new(AtomicUsize::new(0)),
        };

        let err = registry
            .register_backend(" ", Arc::new(factory))
            .expect_err("blank adapter must fail");

        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[test]
    fn normalizes_adapter_registration_name() {
        let mut registry = MessagingRegistry::<String>::new();
        let factory = CountingFactory {
            producer_calls: Arc::new(AtomicUsize::new(0)),
            consumer_calls: Arc::new(AtomicUsize::new(0)),
        };

        registry
            .register_backend(" counting ", Arc::new(factory))
            .unwrap();

        assert!(registry.contains("counting"));
        assert!(!registry.contains(" counting "));
    }

    #[test]
    fn producer_lookup_does_not_construct_consumer() {
        let producer_calls = Arc::new(AtomicUsize::new(0));
        let consumer_calls = Arc::new(AtomicUsize::new(0));
        let factory = CountingFactory {
            producer_calls: Arc::clone(&producer_calls),
            consumer_calls: Arc::clone(&consumer_calls),
        };
        let mut registry = MessagingRegistry::<String>::new();
        registry
            .register_backend("counting", Arc::new(factory))
            .unwrap();

        let _producer = registry.producer(&BrokerConfig::new("counting")).unwrap();

        assert_eq!(producer_calls.load(Ordering::SeqCst), 1);
        assert_eq!(consumer_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn build_constructs_backend_and_consumer_lookup_only_constructs_consumer() {
        let producer_calls = Arc::new(AtomicUsize::new(0));
        let consumer_calls = Arc::new(AtomicUsize::new(0));
        let mut registry = MessagingRegistry::<String>::default();
        registry
            .register_backend(
                "counting",
                Arc::new(CountingFactory {
                    producer_calls: Arc::clone(&producer_calls),
                    consumer_calls: Arc::clone(&consumer_calls),
                }),
            )
            .unwrap();

        assert_eq!(registry.len(), 1);
        assert_eq!(registry.adapters(), vec!["counting"]);
        let backend = registry.build(&BrokerConfig::new("counting")).unwrap();
        let _consumer = registry.consumer(&BrokerConfig::new("counting")).unwrap();

        assert_eq!(producer_calls.load(Ordering::SeqCst), 1);
        assert_eq!(consumer_calls.load(Ordering::SeqCst), 2);
        backend
            .producer
            .send(Message::new("topic", "payload".to_string()))
            .await
            .unwrap();
        backend
            .producer
            .send_batch(vec![Message::new("topic", "payload".to_string())])
            .await
            .unwrap();
        backend.producer.flush(Duration::ZERO).await.unwrap();
        backend.consumer.subscribe(&["topic"]).await.unwrap();
        assert_eq!(
            backend.consumer.recv().await.unwrap_err().code(),
            ErrorCode::NotFound
        );
    }

    #[test]
    fn build_and_consumer_report_unregistered_adapter() {
        let registry = MessagingRegistry::<String>::new();

        assert_eq!(
            registry
                .build(&BrokerConfig::new("missing"))
                .err()
                .map(|error| error.code()),
            Some(ErrorCode::NotFound)
        );
        assert_eq!(
            registry
                .consumer(&BrokerConfig::new("missing"))
                .err()
                .map(|error| error.code()),
            Some(ErrorCode::NotFound)
        );
    }

    struct CountingFactory {
        producer_calls: Arc<AtomicUsize>,
        consumer_calls: Arc<AtomicUsize>,
    }

    impl MessagingFactory<String> for CountingFactory {
        fn create_producer(
            &self,
            _config: &BrokerConfig,
        ) -> AppResult<Arc<dyn MessageProducer<String>>> {
            self.producer_calls.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(NoopProducer))
        }

        fn create_consumer(
            &self,
            _config: &BrokerConfig,
        ) -> AppResult<Arc<dyn MessageConsumer<String>>> {
            self.consumer_calls.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(NoopConsumer))
        }
    }

    struct NoopProducer;

    #[async_trait]
    impl MessageProducer<String> for NoopProducer {
        async fn send(&self, _msg: Message<String>) -> AppResult<()> {
            Ok(())
        }

        async fn send_batch(&self, _msgs: Vec<Message<String>>) -> AppResult<()> {
            Ok(())
        }

        async fn flush(&self, _timeout: Duration) -> AppResult<()> {
            Ok(())
        }
    }

    struct NoopConsumer;

    #[async_trait]
    impl MessageConsumer<String> for NoopConsumer {
        async fn subscribe(&self, _topics: &[&str]) -> AppResult<()> {
            Ok(())
        }

        async fn recv(&self) -> AppResult<Message<String>> {
            Err(AppError::new(ErrorCode::NotFound, "no messages"))
        }
    }
}
