//! Agent context compaction integration.

use rskit_ai::chat::count_tokens_approx;
use rskit_errors::AppResult;
use rskit_llm::types::Message;

use crate::types::{ContextStrategy, FailStrategy};

pub(crate) fn compact_if_needed(
    mut messages: Vec<Message>,
    max_input_tokens: Option<u64>,
    strategy: Option<&dyn ContextStrategy>,
) -> AppResult<Vec<Message>> {
    if let Some(max_input_tokens) = max_input_tokens {
        let limit = max_input_tokens as usize;
        if count_tokens_approx(&messages) > limit {
            let strategy = strategy.unwrap_or(&FailStrategy);
            messages = strategy.compact(messages, limit)?;
        }
    }
    Ok(messages)
}
