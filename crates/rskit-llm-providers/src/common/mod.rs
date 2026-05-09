//! Shared utilities across LLM providers.

mod accumulator;
mod errors;
mod types;

#[cfg(test)]
pub(crate) use accumulator::accumulate_tool_uses;
pub(crate) use accumulator::{parse_input_json, value_to_input_map};
pub use errors::{
    ApiError, estimate_tokens, parse_anthropic_error, parse_gemini_error, parse_openai_error,
};
pub(crate) use types::{StreamChunk, StreamToolCall};
