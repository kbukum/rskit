#![warn(missing_docs)]

//! Shared utilities across LLM provider crates.

mod accumulator;
mod errors;
mod openai;
mod types;

pub use accumulator::{
    accumulate_tool_uses, merge_tool_delta, parse_input_json, value_to_input_map,
};
pub use errors::{
    ApiError, estimate_tokens, parse_anthropic_error, parse_gemini_error, parse_openai_error,
};
pub use openai::OpenAiDialect;
pub use types::{StreamChunk, StreamToolCall};
