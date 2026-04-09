//! Shared utilities across LLM providers.

mod errors;

pub use errors::{
    ApiError, estimate_tokens, parse_anthropic_error, parse_gemini_error, parse_openai_error,
};
