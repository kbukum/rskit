use std::sync::Arc;

use rskit_errors::AppResult;

use crate::{CacheConfig, CacheRegistry, CacheStore, CacheStoreFactory};

use super::{FileCache, FileCacheConfig};

struct FileFactory {
    config: FileCacheConfig,
}

#[async_trait::async_trait]
impl CacheStoreFactory for FileFactory {
    async fn create(&self, config: &CacheConfig) -> AppResult<Arc<dyn CacheStore>> {
        let mut fs = self.config.clone();
        if fs.key_prefix.is_none() {
            fs.key_prefix.clone_from(&config.key_prefix);
        }
        Ok(Arc::new(FileCache::new(fs)))
    }
}

/// Explicitly register the filesystem adapter.
pub fn register_file_cache(registry: &mut CacheRegistry, config: FileCacheConfig) -> AppResult<()> {
    registry.register("fs", Arc::new(FileFactory { config }))
}
