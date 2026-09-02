//! Filesystem cache adapter.

mod config;
mod registration;
mod store;

#[cfg(test)]
pub(crate) use config::DEFAULT_MAX_ENTRY_BYTES;
pub use config::FileCacheConfig;
pub use registration::register_file_cache;
pub use store::FileCache;
#[cfg(test)]
pub(crate) use store::{ENTRY_FORMAT_VERSION, Entry, now_millis, ttl_millis};

#[cfg(test)]
mod tests;
