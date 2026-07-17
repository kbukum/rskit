use std::sync::Arc;

use rskit_errors::AppResult;
use rskit_messaging::{
    BrokerConfigExt, MessageConsumer, MessageProducer, MessagingFactory, MessagingRegistry,
};

use crate::Config;
use crate::consumer::NatsConsumer;
use crate::producer::NatsProducer;

/// Register NATS producer and consumer factories for `Vec<u8>` payloads.
pub fn register(registry: &mut MessagingRegistry<Vec<u8>>, config: Config) -> AppResult<()> {
    config.validate()?;
    if !config.base.enabled {
        return Ok(());
    }
    let adapter = config.base.adapter.clone();
    registry.register_backend(adapter, Arc::new(NatsFactory { config }))
}

pub(crate) struct NatsFactory {
    pub(crate) config: Config,
}

impl MessagingFactory<Vec<u8>> for NatsFactory {
    fn create_producer(
        &self,
        _config: &rskit_messaging::BrokerConfig,
    ) -> AppResult<Arc<dyn MessageProducer<Vec<u8>>>> {
        Ok(Arc::new(NatsProducer::new(self.config.clone())?))
    }

    fn create_consumer(
        &self,
        _config: &rskit_messaging::BrokerConfig,
    ) -> AppResult<Arc<dyn MessageConsumer<Vec<u8>>>> {
        Ok(Arc::new(NatsConsumer::new(self.config.clone())?))
    }
}
