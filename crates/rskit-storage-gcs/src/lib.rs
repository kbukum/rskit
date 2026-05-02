//! Google Cloud Storage backend for `rskit-storage`.
//!
//! Implements [`rskit_storage::FileStore`] for Google Cloud Storage and registers
//! through an explicit [`rskit_storage::StorageRegistry`]. Importing this crate
//! does not register a backend or create a client; applications call
//! [`register_gcs`] with the registry they own.

mod store;

pub use store::{GcsStore, GcsStoreConfig, register_gcs};
