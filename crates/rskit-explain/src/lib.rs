//! Structured explanation generation using LLM providers.

mod explain;
mod types;

pub use explain::generate;
pub use types::{Explanation, ReasoningStep, Request, Signal};
