use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::Deserialize;

// ── Base broker configuration ────────────────────────────────────────────────

/// Configuration shared by all message-broker backends.
///
/// Concrete broker configs (e.g. [`KafkaConfig`]) embed this struct via
/// `#[serde(flatten)]` so that end-users see a single, flat configuration
/// surface while generic code can work through [`BrokerConfigExt`].
#[derive(Debug, Clone, Deserialize)]
pub struct BrokerConfig {
    /// Logical name for this configuration.
    #[serde(default = "default_name")]
    pub name: String,
    /// Whether this configuration is enabled.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Broker addresses (e.g. `["localhost:9092"]`).
    #[serde(default = "default_brokers")]
    pub brokers: Vec<String>,
    /// Number of retries for failed requests.
    #[serde(default = "default_retries")]
    pub retries: u32,
    /// Request timeout in milliseconds (`None` = use broker default).
    #[serde(default)]
    pub request_timeout: Option<u64>,
    /// Default topics to subscribe to.
    #[serde(default)]
    pub topics: Vec<String>,
}

impl BrokerConfig {
    /// Return the request timeout as a [`Duration`], if configured.
    pub fn request_timeout_duration(&self) -> Option<Duration> {
        self.request_timeout.map(Duration::from_millis)
    }
}

impl Default for BrokerConfig {
    fn default() -> Self {
        Self {
            name: default_name(),
            enabled: default_enabled(),
            brokers: default_brokers(),
            retries: default_retries(),
            request_timeout: None,
            topics: Vec::new(),
        }
    }
}

/// Extension trait for broker-specific configurations.
///
/// Every backend configuration struct should implement this so that generic
/// infrastructure (retry policies, health checks, service discovery) can
/// access the common [`BrokerConfig`] and perform validation without knowing
/// the concrete broker type.
pub trait BrokerConfigExt {
    /// Access the shared broker configuration.
    fn base(&self) -> &BrokerConfig;
    /// Validate the complete configuration (base + backend-specific fields).
    fn validate(&self) -> AppResult<()>;
}

// ── Kafka configuration ──────────────────────────────────────────────────────

/// Configuration for connecting to a Kafka cluster.
///
/// The broker-agnostic fields live in the embedded [`BrokerConfig`]
/// (flattened for serde) while Kafka-specific knobs remain here.
#[derive(Debug, Clone, Deserialize)]
pub struct KafkaConfig {
    /// Shared broker settings (name, enabled, brokers, retries, …).
    #[serde(flatten)]
    pub base: BrokerConfig,
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
    /// Security protocol for broker connections.
    #[serde(default)]
    pub security_protocol: SecurityProtocol,
    /// SASL mechanism (e.g. `PLAIN`, `SCRAM-SHA-256`).
    #[serde(default)]
    pub sasl_mechanism: Option<String>,
    /// SASL username.
    #[serde(default)]
    pub sasl_username: Option<String>,
    /// SASL password.
    #[serde(default)]
    pub sasl_password: Option<String>,
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

fn default_retries() -> u32 {
    3
}

fn default_name() -> String {
    "default".to_string()
}

fn default_enabled() -> bool {
    true
}

fn default_brokers() -> Vec<String> {
    vec!["localhost:9092".to_string()]
}

impl Default for KafkaConfig {
    fn default() -> Self {
        Self {
            base: BrokerConfig::default(),
            group_id: None,
            compression: Compression::default(),
            auto_offset_reset: OffsetReset::default(),
            session_timeout: default_session_timeout(),
            batch_size: default_batch_size(),
            linger_ms: default_linger_ms(),
            security_protocol: SecurityProtocol::default(),
            sasl_mechanism: None,
            sasl_username: None,
            sasl_password: None,
        }
    }
}

impl BrokerConfigExt for KafkaConfig {
    fn base(&self) -> &BrokerConfig {
        &self.base
    }

    fn validate(&self) -> AppResult<()> {
        if self.base.brokers.is_empty() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "brokers list cannot be empty",
            ));
        }
        Ok(())
    }
}

impl KafkaConfig {
    /// Build an `rdkafka::config::ClientConfig` from this configuration.
    #[cfg(feature = "kafka")]
    pub fn to_client_config(&self) -> rdkafka::config::ClientConfig {
        let mut cfg = rdkafka::config::ClientConfig::new();
        cfg.set("bootstrap.servers", self.base.brokers.join(","));

        if let Some(ref group) = self.group_id {
            cfg.set("group.id", group);
        }

        let compression = match self.compression {
            Compression::None => "none",
            Compression::Gzip => "gzip",
            Compression::Snappy => "snappy",
            Compression::Lz4 => "lz4",
            Compression::Zstd => "zstd",
        };
        cfg.set("compression.type", compression);

        let offset = match self.auto_offset_reset {
            OffsetReset::Latest => "latest",
            OffsetReset::Earliest => "earliest",
        };
        cfg.set("auto.offset.reset", offset);

        cfg.set(
            "session.timeout.ms",
            self.session_timeout.as_millis().to_string(),
        );
        cfg.set("batch.size", self.batch_size.to_string());
        cfg.set("linger.ms", self.linger_ms.to_string());
        cfg.set("security.protocol", self.security_protocol.to_string());

        if let Some(ref mechanism) = self.sasl_mechanism {
            cfg.set("sasl.mechanism", mechanism);
        }
        if let Some(ref username) = self.sasl_username {
            cfg.set("sasl.username", username);
        }
        if let Some(ref password) = self.sasl_password {
            cfg.set("sasl.password", password);
        }
        if let Some(timeout) = self.base.request_timeout {
            cfg.set("request.timeout.ms", timeout.to_string());
        }
        cfg.set("message.send.max.retries", self.base.retries.to_string());

        cfg
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

/// Security protocol for Kafka broker connections.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub enum SecurityProtocol {
    /// Plaintext (no encryption).
    #[default]
    Plaintext,
    /// SSL/TLS encryption.
    Ssl,
    /// SASL authentication over plaintext.
    SaslPlaintext,
    /// SASL authentication over SSL/TLS.
    SaslSsl,
}

impl fmt::Display for SecurityProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Plaintext => "plaintext",
            Self::Ssl => "ssl",
            Self::SaslPlaintext => "sasl_plaintext",
            Self::SaslSsl => "sasl_ssl",
        };
        f.write_str(s)
    }
}

impl FromStr for SecurityProtocol {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "plaintext" => Ok(Self::Plaintext),
            "ssl" => Ok(Self::Ssl),
            "sasl_plaintext" => Ok(Self::SaslPlaintext),
            "sasl_ssl" => Ok(Self::SaslSsl),
            other => Err(format!("unknown security protocol: {other}")),
        }
    }
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
