use crate::executor::{ChainConfig, ChainExecutor};
use crate::operation::ChainOperation;

/// Fluent builder for constructing chain executors.
pub struct ChainBuilder {
    operations: Vec<Box<dyn ChainOperation>>,
    config: ChainConfig,
}

impl ChainBuilder {
    /// Create a new empty builder.
    pub fn new() -> Self {
        Self {
            operations: Vec::new(),
            config: ChainConfig::default(),
        }
    }

    /// Add an operation to the chain.
    pub fn step(mut self, operation: impl ChainOperation + 'static) -> Self {
        self.operations.push(Box::new(operation));
        self
    }

    /// Set chain configuration.
    pub fn config(mut self, config: ChainConfig) -> Self {
        self.config = config;
        self
    }

    /// Enable or disable cleanup on failure.
    pub fn cleanup_on_failure(mut self, enabled: bool) -> Self {
        self.config.cleanup_on_failure = enabled;
        self
    }

    /// Enable or disable stop-on-failure behavior.
    pub fn stop_on_failure(mut self, enabled: bool) -> Self {
        self.config.stop_on_failure = enabled;
        self
    }

    /// Build the chain executor.
    pub fn build(self) -> ChainExecutor {
        ChainExecutor::new(self.operations).with_config(self.config)
    }
}

impl Default for ChainBuilder {
    fn default() -> Self {
        Self::new()
    }
}
