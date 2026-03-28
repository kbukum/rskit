use std::time::Duration;

use serde::Deserialize;

/// Configuration for connecting to a Kafka cluster.
#[derive(Debug, Clone, Deserialize)]
pub struct KafkaConfig {
    /// Broker addresses (e.g. `["localhost:9092"]`).
    pub brokers: Vec<String>,
    /// Consumer group identifier.
    pub group_id: Option<String>,
    /// Compression algorithm for produced messages.
    #[serde(default)]
    pub compression: Compression,
    /// Where to start consuming when no committed offset exists.
    #[serde(default)]
    pub auto_offset_reset: OffsetReset,
    /// Session timeout for the consumer group.
    #[serde(with = "humantime_serde", default = "default_session_timeout")]
    pub session_timeout: Duration,
    /// Maximum number of messages per batch.
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    /// Delay in milliseconds before sending a batch.
    #[serde(default = "default_linger_ms")]
    pub linger_ms: u64,
}

fn default_session_timeout() -> Duration {
    Duration::from_secs(30)
}

fn default_batch_size() -> usize {
    1000
}

fn default_linger_ms() -> u64 {
    5
}

impl Default for KafkaConfig {
    fn default() -> Self {
        Self {
            brokers: vec!["localhost:9092".to_string()],
            group_id: None,
            compression: Compression::default(),
            auto_offset_reset: OffsetReset::default(),
            session_timeout: default_session_timeout(),
            batch_size: default_batch_size(),
            linger_ms: default_linger_ms(),
        }
    }
}

/// Compression algorithm for produced messages.
#[derive(Debug, Clone, Default, Deserialize)]
pub enum Compression {
    /// No compression.
    #[default]
    None,
    /// Gzip compression.
    Gzip,
    /// Snappy compression.
    Snappy,
    /// LZ4 compression.
    Lz4,
    /// Zstandard compression.
    Zstd,
}

/// Starting offset strategy when no committed offset exists.
#[derive(Debug, Clone, Default, Deserialize)]
pub enum OffsetReset {
    /// Start from the latest (most recent) offset.
    #[default]
    Latest,
    /// Start from the earliest available offset.
    Earliest,
}

/// Serde helper for `Duration` via human-readable strings (e.g. `"30s"`).
mod humantime_serde {
    use std::time::Duration;

    use serde::{Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(secs))
    }
}
