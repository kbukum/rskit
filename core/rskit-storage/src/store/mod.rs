//! File store trait and backends for persistent file storage.

mod key;
mod local;
mod model;
mod progress;
mod registry;
#[cfg(test)]
mod tests;
mod traits;

pub use key::prefixed_key;
pub use local::{LocalStore, LocalStoreConfig};
pub use model::{DEFAULT_CONTENT_TYPE, StoredFile, content_type_or_default};
pub use progress::{ProgressCallback, UploadProgress};
pub use registry::{StorageConfig, StorageFactory, StorageRegistry, register_local};
pub use traits::FileStore;
