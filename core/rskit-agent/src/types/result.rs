use rskit_llm::types::{AssistantMessage, Message, Usage};

use super::StopReason;

/// The final outcome of an agent run.
#[derive(Debug, Clone)]
pub struct AgentResult {
    /// All messages accumulated during the run (user + assistant + tool results).
    pub messages: Vec<Message>,
    /// The last assistant message before the loop ended.
    pub final_message: AssistantMessage,
    /// Aggregate token usage across all turns.
    pub total_usage: Usage,
    /// How many turns the agent executed.
    pub turn_count: u32,
    /// Why the agent stopped.
    pub stop_reason: StopReason,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_llm::types;

    #[test]
    fn test_agent_result_fields() {
        let result = AgentResult {
            messages: vec![types::user("hi")],
            final_message: AssistantMessage {
                content: types::text_content("hello"),
                tool_calls: vec![],
                usage: None,
            },
            total_usage: Usage {
                input_tokens: 10,
                output_tokens: 20,
                cached_tokens: 0,
                reasoning_tokens: 0,
            },
            turn_count: 1,
            stop_reason: StopReason::EndTurn,
        };
        assert_eq!(result.turn_count, 1);
        assert_eq!(result.total_usage.input_tokens, 10);
    }
}
