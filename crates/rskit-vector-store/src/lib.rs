//! Vector store abstraction with Qdrant and in-memory implementations.

mod memory;
mod qdrant;
mod store;

pub use memory::InMemoryVectorStore;
pub use qdrant::{QdrantConfig, QdrantVectorStore};
pub use store::{PointPayload, SearchFilter, SearchResult, VectorStore};
