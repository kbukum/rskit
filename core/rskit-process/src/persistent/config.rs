use std::time::Duration;

use crate::{ProcessSpec, command::DEFAULT_MAX_OUTPUT_BYTES};

/// Persistent process readiness policy.
#[derive(Debug, Clone, Eq, PartialEq)]
#[non_exhaustive]
pub enum PersistentReadiness {
    /// The process is ready immediately after it is spawned.
    Started,
    /// The process is ready when either output stream contains the text.
    OutputContains(String),
    /// The process is ready when a command exits successfully.
    Command(ProcessSpec),
}

/// Configuration for a persistent process.
#[derive(Debug, Clone)]
pub struct PersistentConfig {
    /// Readiness policy used before returning the running process.
    pub readiness: PersistentReadiness,
    /// Maximum time to wait for readiness.
    pub readiness_timeout: Duration,
    /// Maximum time to wait after a graceful shutdown request before killing.
    pub shutdown_grace_period: Duration,
    /// Maximum retained bytes for each captured output stream.
    pub max_capture_bytes: Option<usize>,
    /// Output forwarding policy.
    pub output: PersistentOutput,
}

impl Default for PersistentConfig {
    fn default() -> Self {
        Self {
            readiness: PersistentReadiness::Started,
            readiness_timeout: Duration::from_secs(30),
            shutdown_grace_period: Duration::from_secs(5),
            max_capture_bytes: Some(DEFAULT_MAX_OUTPUT_BYTES),
            output: PersistentOutput::capture_only(),
        }
    }
}

impl PersistentConfig {
    /// Set the readiness policy.
    #[must_use]
    pub fn with_readiness(mut self, readiness: PersistentReadiness) -> Self {
        self.readiness = readiness;
        self
    }

    /// Set the readiness timeout.
    #[must_use]
    pub fn with_readiness_timeout(mut self, timeout: Duration) -> Self {
        self.readiness_timeout = timeout;
        self
    }

    /// Set the shutdown grace period.
    #[must_use]
    pub fn with_shutdown_grace_period(mut self, grace_period: Duration) -> Self {
        self.shutdown_grace_period = grace_period;
        self
    }

    /// Set the maximum retained bytes for each captured output stream.
    #[must_use]
    pub fn with_max_capture_bytes(mut self, bytes: usize) -> Self {
        self.max_capture_bytes = Some(bytes);
        self
    }

    /// Disable capture bounds.
    #[must_use]
    pub fn with_unbounded_capture(mut self) -> Self {
        self.max_capture_bytes = None;
        self
    }

    /// Set the output forwarding policy.
    #[must_use]
    pub fn with_output(mut self, output: PersistentOutput) -> Self {
        self.output = output;
        self
    }
}

/// Persistent process output forwarding policy.
#[derive(Debug, Clone, Copy)]
pub struct PersistentOutput {
    stdout: Option<PersistentOutputStream>,
    stderr: Option<PersistentOutputStream>,
}

impl PersistentOutput {
    /// Capture output without forwarding it to the parent process streams.
    #[must_use]
    pub const fn capture_only() -> Self {
        Self {
            stdout: None,
            stderr: None,
        }
    }

    /// Capture and forward stdout/stderr to the selected parent streams.
    #[must_use]
    pub const fn forward(stdout: PersistentOutputStream, stderr: PersistentOutputStream) -> Self {
        Self {
            stdout: Some(stdout),
            stderr: Some(stderr),
        }
    }

    pub(in crate::persistent) const fn stdout_stream(self) -> Option<PersistentOutputStream> {
        self.stdout
    }

    pub(in crate::persistent) const fn stderr_stream(self) -> Option<PersistentOutputStream> {
        self.stderr
    }
}

/// Parent stream used for forwarding persistent output.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[non_exhaustive]
pub enum PersistentOutputStream {
    /// Forward bytes to parent stdout.
    Stdout,
    /// Forward bytes to parent stderr.
    Stderr,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persistent_config_builders_update_nested_policy() {
        let readiness = PersistentReadiness::OutputContains("ready".to_string());
        let output = PersistentOutput::forward(
            PersistentOutputStream::Stdout,
            PersistentOutputStream::Stderr,
        );
        let config = PersistentConfig::default()
            .with_readiness(readiness.clone())
            .with_readiness_timeout(Duration::from_secs(2))
            .with_shutdown_grace_period(Duration::from_secs(3))
            .with_max_capture_bytes(64)
            .with_output(output);

        assert_eq!(config.readiness, readiness);
        assert_eq!(config.readiness_timeout, Duration::from_secs(2));
        assert_eq!(config.shutdown_grace_period, Duration::from_secs(3));
        assert_eq!(config.max_capture_bytes, Some(64));
        assert_eq!(
            config.output.stdout_stream(),
            Some(PersistentOutputStream::Stdout)
        );
        assert_eq!(
            config.output.stderr_stream(),
            Some(PersistentOutputStream::Stderr)
        );

        assert_eq!(config.with_unbounded_capture().max_capture_bytes, None);
    }
}
