//! Agent runtime configuration.

use std::sync::Arc;
use std::time::Duration;

use rskit_hook::HookRegistry;
use rskit_resilience::Policy;
use rskit_tool::Registry;

use crate::types::ContextStrategy;

/// Configuration for an [`crate::Agent`].
pub struct AgentConfig {
    /// Optional tool registry.
    pub tools: Option<Arc<Registry>>,
    /// Optional hook registry for lifecycle events.
    pub hooks: Option<Arc<HookRegistry>>,
    /// System prompt prepended to every completion request.
    pub system_prompt: String,
    /// Maximum number of turns before the agent stops.
    pub max_turns: u32,
    /// Maximum cumulative token budget (input + output) across all turns.
    pub max_tokens: usize,
    /// Maximum wall-clock time for a run.
    pub wall_clock: Duration,
    /// Maximum logical tool calls. Retries count as one logical call.
    pub max_tool_calls: u32,
    /// Maximum concurrently scheduled tool calls.
    pub tool_concurrency: usize,
    /// Per-tool call timeout.
    pub tool_timeout: Duration,
    /// Optional resilience policy applied to tool executions.
    pub policy: Option<Policy>,
    /// Strategy for compacting context when it exceeds the provider's limit.
    pub context_strategy: Option<Box<dyn ContextStrategy>>,
    /// Model identifier to send with each completion request.
    pub model: String,
}

impl AgentConfig {
    /// Return this configuration as shared GenAI budget vocabulary.
    #[must_use]
    pub fn budget(&self) -> rskit_ai::Budget {
        rskit_ai::Budget {
            max_tokens: Some(self.max_tokens as u64),
            max_calls: Some(u64::from(self.max_tool_calls)),
            max_cost: None,
            wall_clock: Some(self.wall_clock.as_secs()),
        }
    }
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            tools: None,
            hooks: None,
            system_prompt: String::new(),
            max_turns: 10,
            max_tokens: 100_000,
            wall_clock: Duration::from_secs(60),
            max_tool_calls: 50,
            tool_concurrency: 4,
            tool_timeout: Duration::from_secs(30),
            policy: None,
            context_strategy: None,
            model: String::new(),
        }
    }
}
