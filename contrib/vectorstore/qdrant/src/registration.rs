//! Explicit registration of a configured Qdrant backend.

use rskit_errors::AppResult;
use rskit_vectorstore::VectorStoreRegistry;

use crate::config::Config;
use crate::store;

/// Explicitly register a configured Qdrant backend.
pub fn register(registry: &mut VectorStoreRegistry, config: Config) -> AppResult<()> {
    store::register_qdrant(registry, config)
}
