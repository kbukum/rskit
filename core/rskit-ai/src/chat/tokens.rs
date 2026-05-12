//! Approximate token counting helpers.

use super::message::Message;

/// Approximate token count using a coarse 4-chars-per-token heuristic.
#[must_use]
pub fn count_tokens_approx(messages: &[Message]) -> usize {
    messages
        .iter()
        .map(|message| match message {
            Message::User(user) => {
                user.content
                    .iter()
                    .map(|part| part.approx_chars() / 4 + 1)
                    .sum::<usize>()
                    + 4
            }
            Message::Assistant(assistant) => {
                assistant
                    .content
                    .iter()
                    .map(|part| part.approx_chars())
                    .sum::<usize>()
                    / 4
                    + assistant.tool_calls.len()
                    + 4
            }
            Message::System(system) => system.content.len() / 4 + 4,
            Message::Tool(tool) => tool.content.len() / 4 + 4,
        })
        .sum()
}
