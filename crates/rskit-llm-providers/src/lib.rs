//! LLM provider implementations (OpenAI, Anthropic, Gemini) for rskit.
//!
//! Each sub-module provides a vendor-specific [`rskit_llm::Provider`]
//! implementation together with a `Config` struct and an `new_adapter()`
//! factory that wires up [`rskit_httpclient`] with the correct auth strategy.

pub mod anthropic;
pub mod common;
pub mod gemini;
pub mod ollama;
pub mod openai;
