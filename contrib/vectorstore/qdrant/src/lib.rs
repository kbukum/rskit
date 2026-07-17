//! Qdrant adapter for [`rskit_vectorstore`].

#![warn(missing_docs)]

mod config;
mod conversion;
mod registration;
mod store;
mod url;

pub use config::Config;
pub use registration::register;
