//! Component registry configuration.

use std::time::Duration;

/// Configuration for the component registry.
#[derive(Debug, Clone)]
pub struct RegistryConfig {
    /// Maximum number of components to start in parallel.
    /// `0` means start all concurrently without any limit.
    pub concurrency: usize,
    /// Timeout applied to each component's `start()` call.
    pub start_timeout: Duration,
    /// Timeout applied to each component's `stop()` call.
    pub stop_timeout: Duration,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            concurrency: 1,
            start_timeout: Duration::from_secs(30),
            stop_timeout: Duration::from_secs(30),
        }
    }
}
