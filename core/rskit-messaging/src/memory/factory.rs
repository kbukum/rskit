//! Registry factory that serves producers and consumers from a shared broker.

use std::sync::Arc;

use rskit_errors::AppResult;

use super::broker::InMemoryBroker;
use crate::config::BrokerConfig;
use crate::registry::{MessagingFactory, MessagingRegistry};
use crate::traits::{MessageConsumer, MessageProducer};

const ADAPTER_NAME: &str = "memory";

/// Register in-memory producer and consumer factories.
pub fn register<T: Clone + Send + Sync + 'static>(
    registry: &mut MessagingRegistry<T>,
    broker: InMemoryBroker<T>,
) -> AppResult<()> {
    registry.register_backend(ADAPTER_NAME, Arc::new(MemoryFactory { broker }))
}

struct MemoryFactory<T: Clone + Send + Sync + 'static> {
    broker: InMemoryBroker<T>,
}

impl<T: Clone + Send + Sync + 'static> MessagingFactory<T> for MemoryFactory<T> {
    fn create_producer(&self, _config: &BrokerConfig) -> AppResult<Arc<dyn MessageProducer<T>>> {
        Ok(Arc::new(self.broker.producer()))
    }

    fn create_consumer(&self, _config: &BrokerConfig) -> AppResult<Arc<dyn MessageConsumer<T>>> {
        Ok(Arc::new(self.broker.consumer()))
    }
}
