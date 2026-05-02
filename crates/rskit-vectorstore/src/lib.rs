//! Vector store abstraction with in-memory default and opt-in adapters.

mod memory;
#[cfg(feature = "qdrant")]
mod qdrant;
mod registry;
mod store;

pub use memory::InMemoryVectorStore;
#[cfg(feature = "qdrant")]
pub use qdrant::{QdrantConfig, QdrantVectorStore, register_qdrant};
pub use registry::{VectorFactory, VectorStoreRegistry, register_memory};
pub use store::{
    FilterCondition, PointPayload, SearchFilter, SearchResult, SimilarityMetric, VectorStore,
};
