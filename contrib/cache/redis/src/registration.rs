use std::sync::Arc;

use rskit_cache::{CacheConfig, CacheRegistry, CacheStore, CacheStoreFactory};
use rskit_errors::AppResult;

use super::{Config, RedisClient};

pub(crate) struct RedisFactory {
    pub(crate) config: Config,
}

#[async_trait::async_trait]
impl CacheStoreFactory for RedisFactory {
    async fn create(&self, config: &CacheConfig) -> AppResult<Arc<dyn CacheStore>> {
        let mut redis = self.config.clone();
        if redis.key_prefix.is_none() {
            redis.key_prefix.clone_from(&config.key_prefix);
        }
        Ok(Arc::new(RedisClient::new(redis).await?))
    }
}

/// Explicitly register the Redis cache store.
pub fn register(registry: &mut CacheRegistry, config: Config) -> AppResult<()> {
    registry.register("redis", Arc::new(RedisFactory { config }))
}
