//! Write capability: persist or patch configuration back to a backend.
//!
//! [`ConfigSink`] is the framework-agnostic write contract; concrete backends
//! (an in-memory store, a file, a future Vault/SSM adapter) implement it and are
//! injected explicitly. Two reference implementations ship here:
//!
//! - [`InMemoryConfigSink`] — a process-local store, also a
#![cfg_attr(
    feature = "watch",
    doc = "  [`ConfigWatch`](crate::ConfigWatch) source under the `watch` feature."
)]
#![cfg_attr(
    not(feature = "watch"),
    doc = "  `ConfigWatch` source under the `watch` feature."
)]
//! - [`FileConfigSink`] — a file-backed store with a pluggable on-disk
//!   [`Codec`](rskit_codec::Codec) (TOML by default).
//!
//! Values flow as [`SecretString`](rskit_util::SecretString) end-to-end and are
//! never logged.

mod contract;
mod file;
mod memory;

pub use contract::ConfigSink;
pub use file::{ConfigTable, FileConfigSink};
pub use memory::InMemoryConfigSink;
