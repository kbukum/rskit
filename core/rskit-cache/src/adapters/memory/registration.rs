use std::sync::Arc;

use rskit_errors::AppResult;

use crate::config::CacheConfig;
use crate::registry::{CacheRegistry, CacheStore, CacheStoreFactory};

use super::MemoryCache;

struct MemoryFactory;

#[async_trait::async_trait]
impl CacheStoreFactory for MemoryFactory {
    async fn create(&self, config: &CacheConfig) -> AppResult<Arc<dyn CacheStore>> {
        Ok(Arc::new(MemoryCache::new(
            config.key_prefix.clone(),
            config.memory.max_entries,
        )))
    }
}

/// Explicitly register the in-memory adapter.
pub fn register_memory(registry: &mut CacheRegistry) -> AppResult<()> {
    registry.register("memory", Arc::new(MemoryFactory))
}
