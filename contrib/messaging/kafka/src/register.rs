use std::sync::Arc;

use rskit_errors::AppResult;
use rskit_messaging::{
    BrokerConfigExt, MessageConsumer, MessageProducer, MessagingFactory, MessagingRegistry,
};

use crate::Config;
use crate::consumer::KafkaConsumer;
use crate::producer::KafkaProducer;

/// Register Kafka producer and consumer factories for `Vec<u8>` payloads.
pub fn register(registry: &mut MessagingRegistry<Vec<u8>>, config: Config) -> AppResult<()> {
    config.validate()?;
    if !config.base.enabled {
        return Ok(());
    }
    let adapter = config.base.adapter.clone();
    registry.register_backend(adapter, Arc::new(KafkaFactory { config }))
}

pub(crate) struct KafkaFactory {
    pub(crate) config: Config,
}

impl MessagingFactory<Vec<u8>> for KafkaFactory {
    fn create_producer(
        &self,
        _config: &rskit_messaging::BrokerConfig,
    ) -> AppResult<Arc<dyn MessageProducer<Vec<u8>>>> {
        Ok(Arc::new(KafkaProducer::new(&self.config)?))
    }

    fn create_consumer(
        &self,
        _config: &rskit_messaging::BrokerConfig,
    ) -> AppResult<Arc<dyn MessageConsumer<Vec<u8>>>> {
        Ok(Arc::new(KafkaConsumer::new(&self.config)?))
    }
}
