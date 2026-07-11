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
        let limit = usize::try_from(max_input_tokens).unwrap_or(usize::MAX);
        if count_tokens_approx(&messages) > limit {
            let strategy = strategy.unwrap_or(&FailStrategy);
            messages = strategy.compact(messages, limit)?;
        }
    }
    Ok(messages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_errors::ErrorCode;

    #[test]
    fn compact_if_needed_leaves_messages_with_no_limit_or_within_limit() {
        let messages = vec![rskit_llm::types::user("short")];

        assert_eq!(
            compact_if_needed(messages.clone(), None, None)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            compact_if_needed(messages, Some(10_000), None)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn compact_if_needed_uses_strategy_when_limit_is_exceeded() {
        struct DropAll;
        impl ContextStrategy for DropAll {
            fn compact(
                &self,
                _messages: Vec<Message>,
                _max_tokens: usize,
            ) -> AppResult<Vec<Message>> {
                Ok(vec![rskit_llm::types::system("summary")])
            }
        }

        let messages = vec![rskit_llm::types::user("this message is intentionally long")];
        let compacted = compact_if_needed(messages, Some(1), Some(&DropAll)).unwrap();

        assert_eq!(compacted, vec![rskit_llm::types::system("summary")]);
    }

    #[test]
    fn compact_if_needed_defaults_to_fail_strategy_when_over_limit() {
        let err =
            compact_if_needed(vec![rskit_llm::types::user("too long")], Some(1), None).unwrap_err();

        assert_eq!(err.code(), ErrorCode::InvalidInput);
        assert!(err.to_string().contains("context"));
    }
}
