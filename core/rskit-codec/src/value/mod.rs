//! Value-tree operations over the canonical [`serde_json::Value`] model.

mod merge;

pub use merge::{ArrayStrategy, merge, merge_with};
