//! Local filesystem storage backend.

mod config;
mod path;
mod store;

#[cfg(test)]
mod tests;

pub use config::LocalStoreConfig;
pub use store::LocalStore;
