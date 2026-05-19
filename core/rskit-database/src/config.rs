//! Database configuration types.

use std::time::Duration;

use rskit_validation::Validate;
use serde::{Deserialize, Deserializer, Serialize};

/// Config-driven database backend selection.
#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
pub struct DatabaseConfig {
    /// Backend name looked up in an injected [`crate::DatabaseRegistry`].
    #[serde(default = "default_backend")]
    pub backend: String,
    /// In-memory backend options.
    #[serde(default)]
    pub memory: MemoryDatabaseConfig,
    /// Maximum number of logical connections in the selected backend.
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    /// Minimum number of idle logical connections to maintain.
    #[serde(default = "default_min_connections")]
    pub min_connections: u32,
    /// Timeout for establishing a backend connection.
    #[serde(
        default = "default_connect_timeout",
        deserialize_with = "deserialize_duration_secs"
    )]
    pub connect_timeout: Duration,
    /// Queries slower than this threshold are reported by backends that support slow-query logging.
    #[serde(
        default = "default_slow_query_threshold",
        deserialize_with = "deserialize_duration_secs"
    )]
    pub slow_query_threshold: Duration,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            backend: default_backend(),
            memory: MemoryDatabaseConfig::default(),
            max_connections: default_max_connections(),
            min_connections: default_min_connections(),
            connect_timeout: default_connect_timeout(),
            slow_query_threshold: default_slow_query_threshold(),
        }
    }
}

/// In-memory database backend configuration.
#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
pub struct MemoryDatabaseConfig {
    /// Logical database name used for diagnostics.
    #[serde(default = "default_memory_name")]
    pub name: String,
    /// Maximum number of recorded statements kept for diagnostics. `0` disables recording.
    #[serde(default = "default_statement_history")]
    pub statement_history: usize,
}

impl Default for MemoryDatabaseConfig {
    fn default() -> Self {
        Self {
            name: default_memory_name(),
            statement_history: default_statement_history(),
        }
    }
}

fn default_backend() -> String {
    "memory".to_owned()
}

fn default_memory_name() -> String {
    "default".to_owned()
}

fn default_statement_history() -> usize {
    256
}

fn default_max_connections() -> u32 {
    10
}

fn default_min_connections() -> u32 {
    1
}

fn default_connect_timeout() -> Duration {
    Duration::from_secs(30)
}

fn default_slow_query_threshold() -> Duration {
    Duration::from_secs(1)
}

fn deserialize_duration_secs<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let secs = u64::deserialize(deserializer)?;
    Ok(Duration::from_secs(secs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_selects_memory() {
        let cfg = DatabaseConfig::default();

        assert_eq!(cfg.backend, "memory");
        assert_eq!(cfg.memory.name, "default");
        assert_eq!(cfg.memory.statement_history, 256);
        assert_eq!(cfg.max_connections, 10);
        assert_eq!(cfg.min_connections, 1);
        assert_eq!(cfg.connect_timeout, Duration::from_secs(30));
        assert_eq!(cfg.slow_query_threshold, Duration::from_secs(1));
    }

    #[test]
    fn deserialize_config_from_json() {
        let json = r#"{
            "backend": "memory",
            "memory": {"name": "testdb", "statement_history": 32}
        }"#;
        let cfg: DatabaseConfig = serde_json::from_str(json).unwrap();

        assert_eq!(cfg.backend, "memory");
        assert_eq!(cfg.memory.name, "testdb");
        assert_eq!(cfg.memory.statement_history, 32);
    }
}
