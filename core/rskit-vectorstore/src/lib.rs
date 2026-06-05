//! Vector store abstraction with in-memory default and opt-in adapters.

#![warn(missing_docs)]

mod config;
mod memory;
mod registry;
mod store;

pub use config::{
    DEFAULT_MAX_FILTER_CONDITIONS, DEFAULT_MAX_PAYLOAD_BYTES, DEFAULT_MAX_PAYLOAD_FIELDS,
    DEFAULT_MAX_SEARCH_LIMIT, DEFAULT_MAX_VECTOR_DIMENSIONS, MemoryVectorStoreConfig,
    VectorStoreConfig, VectorStoreLimits,
};
pub use memory::InMemoryVectorStore;
pub use registry::{VectorFactory, VectorStoreRegistry, register_memory};
pub use store::{
    FilterCondition, PayloadValue, PointPayload, SearchFilter, SearchResult, SimilarityMetric,
    VectorStore,
};
