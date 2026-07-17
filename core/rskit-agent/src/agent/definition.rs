use std::sync::Arc;

use rskit_llm::provider::Provider;

use crate::config::AgentConfig;

/// A multi-turn agentic loop that drives an LLM provider, executes tool calls,
/// and emits hook events at each lifecycle point.
pub struct Agent {
    pub(super) provider: Arc<dyn Provider>,
    pub(super) config: AgentConfig,
}

impl Agent {
    /// Create a new agent with the given provider and configuration.
    pub fn new(provider: Arc<dyn Provider>, config: AgentConfig) -> Self {
        Self { provider, config }
    }

    /// Create a new agent with the locked default configuration.
    pub fn with_defaults(provider: Arc<dyn Provider>) -> Self {
        Self::new(provider, AgentConfig::default())
    }
}
