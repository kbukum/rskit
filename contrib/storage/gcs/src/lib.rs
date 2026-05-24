//! Google Cloud Storage backend for [`rskit_storage`].
//!
//! This crate implements [`rskit_storage::store::FileStore`] without adding
//! Google Cloud dependencies to the core storage crate. Importing it has no
//! side effects; applications call [`register`] with the registry they own.

#![warn(missing_docs)]

mod store;

pub use store::{Config, register};
