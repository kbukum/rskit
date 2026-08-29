#![warn(missing_docs)]

//! `OpenAI` BPE (tiktoken) [`rskit_llm::TokenCounter`] adapter.
//!
//! Wraps [`tiktoken-rs`](https://docs.rs/tiktoken-rs) to provide exact `OpenAI`
//! token counts for a chosen encoding. The encoding is selected explicitly at
//! construction and the BPE ranks are bundled in the dependency, so the adapter
//! performs no network access and has no import-time side effects.

mod config;
mod counter;

pub use config::Encoding;
pub use counter::{TiktokenCounter, counter};
