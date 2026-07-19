//! Amazon S3 and S3-compatible (MinIO, LocalStack) storage backend.
//!
//! Registers a [`rskit_storage::FileStore`] implementation for Amazon S3
//! and any S3-compatible object store (MinIO, LocalStack, Ceph, etc.).
//!
//! # Features
//!
//! - **MinIO / S3-compatible**: `force_path_style`, custom endpoint, explicit credentials
//! - **Standard FileStore trait**: drop-in replacement for any rskit-storage backend
//!
//! # Credential Resolution
//!
//! Credentials are resolved in this order:
//! 1. Explicit `access_key_id` + `secret_access_key` from [`Config`]
//! 2. `AWS_ACCESS_KEY_ID` + `AWS_SECRET_ACCESS_KEY` environment variables
//!
//! # Example
//!
//! ```rust,no_run
//! use rskit_storage::StorageRegistry;
//! use rskit_storage_s3::{Config, register};
//!
//! # async fn example() -> rskit_errors::AppResult<()> {
//! let mut registry = StorageRegistry::new();
//! register(&mut registry, Config {
//!     bucket: "my-assets".into(),
//!     region: Some("us-east-1".into()),
//!     endpoint: Some("http://localhost:9000".into()),
//!     force_path_style: true,
//!     access_key_id: Some("minioadmin".into()),
//!     secret_access_key: Some("minioadmin".into()),
//!     prefix: None,
//! })?;
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]

mod store;

pub use store::{Config, register};
