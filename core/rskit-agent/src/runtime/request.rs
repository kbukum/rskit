//! Completion request construction for agent turns.

use rskit_llm::types::{CompletionRequest, Message, ToolDefinition};
use rskit_tool::Registry;

use crate::config::AgentConfig;

pub(crate) fn build_completion_request(
    config: &AgentConfig,
    messages: &[Message],
) -> CompletionRequest {
    CompletionRequest {
        model: config.model.clone(),
        messages: messages.to_vec(),
        max_tokens: None,
        temperature: None,
        stream: false,
        tools: config
            .tools
            .as_ref()
            .map(|registry| tool_definitions(registry.as_ref())),
        tool_choice: None,
    }
}

fn tool_definitions(registry: &Registry) -> Vec<ToolDefinition> {
    registry
        .list()
        .into_iter()
        .map(|tool| ToolDefinition {
            name: tool.name,
            description: tool.description,
            input_schema: tool.input_schema,
            output_schema: tool.output_schema,
        })
        .collect()
}
