//! Qdrant adapter for [`rskit_vectorstore`].

#![warn(missing_docs)]

mod config;
mod conversion;
mod store;
mod url;

use rskit_errors::AppResult;
use rskit_vectorstore::VectorStoreRegistry;

pub use config::Config;

/// Explicitly register a configured Qdrant backend.
pub fn register(registry: &mut VectorStoreRegistry, config: Config) -> AppResult<()> {
    store::register_qdrant(registry, config)
}
