//! Aggregate configuration for subprocess execution behavior.

use std::time::Duration;

use super::io::{InputPolicy, ProcessIo};
use super::lifecycle::LifecyclePolicy;
use super::redaction::ArgRedaction;

/// Configuration for subprocess execution behavior.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ProcessConfig {
    /// Overall timeout for the process. None means no timeout.
    pub timeout: Option<Duration>,
    /// Explicit I/O strategy.
    pub io: ProcessIo,
    /// Child lifecycle, isolation, and termination policy.
    pub lifecycle: LifecyclePolicy,
    /// Redaction policy for command-line arguments emitted in spawn logs.
    pub arg_redaction: ArgRedaction,
}

impl Default for ProcessConfig {
    fn default() -> Self {
        Self {
            timeout: Some(Duration::from_secs(30)),
            io: ProcessIo::default(),
            lifecycle: LifecyclePolicy::default(),
            arg_redaction: ArgRedaction::default(),
        }
    }
}

impl ProcessConfig {
    /// Set the process timeout.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set the I/O strategy.
    #[must_use]
    pub fn with_io(mut self, io: ProcessIo) -> Self {
        self.io = io;
        self
    }

    /// Set the lifecycle policy.
    #[must_use]
    pub fn with_lifecycle_policy(mut self, lifecycle: LifecyclePolicy) -> Self {
        self.lifecycle = lifecycle;
        self
    }

    /// Set the command-line argument redaction policy used for spawn logs.
    #[must_use]
    pub fn with_arg_redaction(mut self, arg_redaction: ArgRedaction) -> Self {
        self.arg_redaction = arg_redaction;
        self
    }

    /// Add a custom secret-bearing command-line argument name for spawn-log redaction.
    #[must_use]
    pub fn with_sensitive_arg_name(mut self, name: impl AsRef<str>) -> Self {
        self.arg_redaction = self.arg_redaction.with_name(name);
        self
    }

    /// Set the maximum retained bytes for captured or observed output.
    #[must_use]
    pub fn with_max_output_bytes(mut self, bytes: usize) -> Self {
        match &mut self.io {
            ProcessIo::Captured(io) => io.output.max_output_bytes = Some(bytes),
            ProcessIo::Observed(io) => io.output.max_output_bytes = Some(bytes),
            #[cfg(unix)]
            ProcessIo::Pty(io) => io.output.max_output_bytes = Some(bytes),
            ProcessIo::Inherited(_) => {}
        }
        self
    }

    /// Disable output capture bounds for captured or observed output.
    #[must_use]
    pub fn with_unbounded_output(mut self) -> Self {
        match &mut self.io {
            ProcessIo::Captured(io) => io.output.max_output_bytes = None,
            ProcessIo::Observed(io) => io.output.max_output_bytes = None,
            #[cfg(unix)]
            ProcessIo::Pty(io) => io.output.max_output_bytes = None,
            ProcessIo::Inherited(_) => {}
        }
        self
    }

    /// Set stdin for the configured I/O mode.
    #[must_use]
    pub fn with_input(mut self, input: InputPolicy) -> Self {
        match &mut self.io {
            ProcessIo::Captured(io) => io.input = input,
            ProcessIo::Observed(io) => io.input = input,
            #[cfg(unix)]
            ProcessIo::Pty(io) => io.input = input,
            ProcessIo::Inherited(io) => io.input = input,
        }
        self
    }
}
