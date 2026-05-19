//! Vector store abstraction with in-memory default and opt-in adapters.

#![warn(missing_docs)]

mod config;
mod memory;
mod registry;
mod store;

pub use config::{MemoryVectorStoreConfig, VectorStoreConfig};
pub use memory::InMemoryVectorStore;
pub use registry::{VectorFactory, VectorStoreRegistry, register_memory};
pub use store::{
    FilterCondition, PointPayload, SearchFilter, SearchResult, SimilarityMetric, VectorStore,
};
