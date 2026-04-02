//! Inference provider abstraction and OpenAI-compatible implementation.

mod openai;
mod provider;

pub use openai::{OpenAiInferenceConfig, OpenAiInferenceProvider};
pub use provider::{CompletionRequest, CompletionResponse, InferenceProvider, Message};
