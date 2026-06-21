//! Dynamic reload capability: observe backend changes and re-decode.
//!
//! [`ConfigWatch`] lets a long-running consumer observe backend changes and
//! re-run the load pipeline + re-decode when configuration changes. One-shot
//! tools and CLIs simply never link this layer — the whole module is gated
//! behind the `watch` feature.

mod contract;

pub use contract::{ConfigChange, ConfigChangeStream, ConfigWatch};
