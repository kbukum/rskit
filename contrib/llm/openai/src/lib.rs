#![warn(missing_docs)]

//! `OpenAI` provider (chat completions + embeddings).

mod adapter;
mod config;
mod dialect;
mod embedding;

#[cfg(test)]
mod fixture_tests;

pub use adapter::register;
pub use config::Config;

#[doc(hidden)]
pub mod __private {
    pub use crate::dialect::OpenAiDialect;
    pub use crate::embedding::EmbeddingProvider;
}
