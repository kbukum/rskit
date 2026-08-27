#![warn(missing_docs)]

//! `HuggingFace` [`rskit_llm::TokenCounter`] adapter.
//!
//! Wraps the [`tokenizers`](https://docs.rs/tokenizers) crate to count tokens
//! with any `HuggingFace` `tokenizer.json`. The tokenizer is loaded explicitly
//! from a caller-supplied path or an in-memory definition — nothing is
//! downloaded and there are no import-time side effects.

mod counter;

pub use counter::{HfTokenCounter, counter};
