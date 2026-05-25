//! Private run-state owned by the synchronous agent loop.

use std::time::Duration;

use rskit_errors::AppResult;
use rskit_llm::types::{AssistantMessage, CompletionResponse, Message, Usage};
use tokio::time::Instant;

use crate::context::compact_if_needed;
use crate::types::{AgentResult, ContextStrategy, StopReason};

pub(crate) struct RunState {
    pub(crate) messages: Vec<Message>,
    pub(crate) total_usage: Usage,
    pub(crate) last_assistant: AssistantMessage,
    pub(crate) tool_calls_used: u32,
    started_at: Instant,
}

impl RunState {
    pub(crate) fn new(system_prompt: &str, messages: Vec<Message>) -> Self {
        let mut all_messages = vec![rskit_llm::system(system_prompt)];
        all_messages.extend(messages);

        Self {
            messages: all_messages,
            total_usage: Usage {
                input_tokens: 0,
                output_tokens: 0,
                cached_tokens: 0,
                reasoning_tokens: 0,
            },
            last_assistant: AssistantMessage {
                content: vec![],
                tool_calls: vec![],
                usage: None,
            },
            tool_calls_used: 0,
            started_at: Instant::now(),
        }
    }

    pub(crate) fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }

    pub(crate) fn remaining_wall_clock(&self, wall_clock: Duration) -> Duration {
        wall_clock.saturating_sub(self.elapsed())
    }

    pub(crate) fn record_response(&mut self, response: CompletionResponse) {
        self.total_usage.input_tokens += response.usage.input_tokens;
        self.total_usage.output_tokens += response.usage.output_tokens;
        self.last_assistant = response.message.clone();
        self.messages.push(Message::Assistant(response.message));
    }

    pub(crate) fn total_tokens(&self) -> usize {
        (self.total_usage.input_tokens + self.total_usage.output_tokens) as usize
    }

    pub(crate) fn compact_context(
        &mut self,
        max_input_tokens: Option<u64>,
        strategy: Option<&dyn ContextStrategy>,
    ) -> AppResult<()> {
        let messages = std::mem::take(&mut self.messages);
        self.messages = compact_if_needed(messages, max_input_tokens, strategy)?;
        Ok(())
    }

    pub(crate) fn finish(self, turn_count: u32, stop_reason: StopReason) -> AgentResult {
        AgentResult {
            messages: self.messages,
            final_message: self.last_assistant,
            total_usage: self.total_usage,
            turn_count,
            stop_reason,
        }
    }
}
