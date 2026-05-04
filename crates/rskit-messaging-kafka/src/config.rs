use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_messaging::{
    BrokerConfig, BrokerConfigExt, CommitStrategy, DeliveryGuarantee, DlqPolicy,
};
use serde::{Deserialize, Deserializer};

pub(crate) const BACKEND_NAME: &str = "kafka";

/// Configuration for connecting to a Kafka cluster.
///
/// Broker-agnostic fields live in [`BrokerConfig`]. This adapter owns only Kafka client
/// knobs: bootstrap servers, compression, offset reset, batching, and security settings.
#[derive(Clone)]
pub struct KafkaConfig {
    /// Shared broker settings (backend/name/enabled, delivery, retry, DLQ, etc.).
    pub base: BrokerConfig,
    /// Kafka bootstrap servers. Credentials and query strings are rejected; use SASL fields.
    pub brokers: Vec<String>,
    /// Compression algorithm for produced messages.
    pub compression: Compression,
    /// Where to start consuming when no committed offset exists.
    pub auto_offset_reset: OffsetReset,
    /// Session timeout for the consumer group, in seconds.
    pub session_timeout: Duration,
    /// Maximum number of messages per batch.
    pub batch_size: usize,
    /// Delay in milliseconds before sending a batch.
    pub linger_ms: u64,
    /// Security protocol for broker connections.
    pub security_protocol: SecurityProtocol,
    /// Permit plaintext connections for explicit local-development use only.
    pub allow_insecure_dev: bool,
    /// SASL mechanism (e.g. `PLAIN`, `SCRAM-SHA-256`).
    pub sasl_mechanism: Option<String>,
    /// SASL username.
    pub sasl_username: Option<String>,
    /// SASL password.
    pub sasl_password: Option<String>,
}

#[derive(Deserialize)]
struct KafkaConfigSerde {
    #[serde(default, flatten)]
    base: BrokerConfig,
    #[serde(default = "default_brokers")]
    brokers: Vec<String>,
    #[serde(default)]
    compression: Compression,
    #[serde(default)]
    auto_offset_reset: OffsetReset,
    #[serde(with = "duration_seconds", default = "default_session_timeout")]
    session_timeout: Duration,
    #[serde(default = "default_batch_size")]
    batch_size: usize,
    #[serde(default = "default_linger_ms")]
    linger_ms: u64,
    #[serde(default)]
    security_protocol: SecurityProtocol,
    #[serde(default)]
    allow_insecure_dev: bool,
    #[serde(default)]
    sasl_mechanism: Option<String>,
    #[serde(default)]
    sasl_username: Option<String>,
    #[serde(default)]
    sasl_password: Option<String>,
}

impl<'de> Deserialize<'de> for KafkaConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut config = KafkaConfigSerde::deserialize(deserializer)?;
        apply_adapter_base_defaults(&mut config.base);
        Ok(Self {
            base: config.base,
            brokers: config.brokers,
            compression: config.compression,
            auto_offset_reset: config.auto_offset_reset,
            session_timeout: config.session_timeout,
            batch_size: config.batch_size,
            linger_ms: config.linger_ms,
            security_protocol: config.security_protocol,
            allow_insecure_dev: config.allow_insecure_dev,
            sasl_mechanism: config.sasl_mechanism,
            sasl_username: config.sasl_username,
            sasl_password: config.sasl_password,
        })
    }
}

impl fmt::Debug for KafkaConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let redacted_brokers = self
            .brokers
            .iter()
            .map(|broker| redact_uri_credentials(broker))
            .collect::<Vec<_>>();

        f.debug_struct("KafkaConfig")
            .field("backend", &self.base.backend)
            .field("name", &self.base.name)
            .field("enabled", &self.base.enabled)
            .field("brokers", &redacted_brokers)
            .field("retries", &self.base.retries)
            .field("retry_backoff", &self.base.retry_backoff)
            .field("request_timeout", &self.base.request_timeout)
            .field("delivery_guarantee", &self.base.delivery_guarantee)
            .field("commit_strategy", &self.base.commit_strategy)
            .field("dlq", &self.base.dlq)
            .field("max_in_flight", &self.base.max_in_flight)
            .field("consumer_group", &self.base.consumer_group)
            .field("topics", &self.base.topics)
            .field("subscriptions", &self.base.subscriptions)
            .field("compression", &self.compression)
            .field("auto_offset_reset", &self.auto_offset_reset)
            .field("session_timeout", &self.session_timeout)
            .field("batch_size", &self.batch_size)
            .field("linger_ms", &self.linger_ms)
            .field("security_protocol", &self.security_protocol)
            .field("allow_insecure_dev", &self.allow_insecure_dev)
            .field("sasl_mechanism", &self.sasl_mechanism)
            .field(
                "sasl_username",
                &redacted_option(self.sasl_username.as_ref()),
            )
            .field(
                "sasl_password",
                &redacted_option(self.sasl_password.as_ref()),
            )
            .finish()
    }
}

impl Default for KafkaConfig {
    fn default() -> Self {
        Self {
            base: default_kafka_base(),
            brokers: default_brokers(),
            compression: Compression::default(),
            auto_offset_reset: OffsetReset::default(),
            session_timeout: default_session_timeout(),
            batch_size: default_batch_size(),
            linger_ms: default_linger_ms(),
            security_protocol: SecurityProtocol::default(),
            allow_insecure_dev: false,
            sasl_mechanism: None,
            sasl_username: None,
            sasl_password: None,
        }
    }
}

impl KafkaConfig {
    pub(crate) fn effective_group_id(&self) -> Option<&str> {
        self.base.consumer_group.as_deref()
    }
}

impl BrokerConfigExt for KafkaConfig {
    fn base(&self) -> &BrokerConfig {
        &self.base
    }

    fn validate(&self) -> AppResult<()> {
        self.base.validate()?;
        validate_backend(&self.base.backend)?;

        if self.base.dlq.enabled {
            return invalid(
                "Kafka adapter does not implement DLQ routing; disable base.dlq.enabled or add DLQ middleware",
            );
        }
        if !matches!(self.base.commit_strategy, CommitStrategy::Auto) {
            return invalid(
                "Kafka adapter direct consumers require commit_strategy=auto until ack-aware consumers are available",
            );
        }
        if matches!(self.base.delivery_guarantee, DeliveryGuarantee::ExactlyOnce) {
            return invalid(
                "Kafka exactly_once delivery requires transactional support that is not implemented",
            );
        }
        if self.brokers.is_empty() {
            return invalid("Kafka brokers list cannot be empty");
        }
        if self.brokers.iter().any(|broker| broker.trim().is_empty()) {
            return invalid("Kafka brokers must not contain empty entries");
        }
        if self
            .brokers
            .iter()
            .any(|broker| has_url_credentials(broker) || broker.contains('?'))
        {
            return invalid("Kafka broker addresses must not contain credentials or query strings");
        }
        for topic in self
            .base
            .topics
            .iter()
            .chain(self.base.subscriptions.iter())
        {
            validate_topic("Kafka topic", topic)?;
        }
        if let Some(group_id) = self.base.consumer_group.as_ref() {
            validate_topic("Kafka consumer_group", group_id)?;
        }
        if self.batch_size == 0 {
            return invalid("Kafka batch_size must be greater than zero");
        }
        if self.session_timeout.is_zero() {
            return invalid("Kafka session_timeout must be greater than zero");
        }
        if matches!(
            self.security_protocol,
            SecurityProtocol::Plaintext | SecurityProtocol::SaslPlaintext
        ) && !self.allow_insecure_dev
        {
            return invalid("Kafka plaintext protocols require allow_insecure_dev=true");
        }
        if matches!(
            self.security_protocol,
            SecurityProtocol::SaslPlaintext | SecurityProtocol::SaslSsl
        ) && (self.sasl_username.is_none() || self.sasl_password.is_none())
        {
            return invalid("Kafka SASL security requires username and password");
        }
        Ok(())
    }
}

/// Compression algorithm for produced messages.
#[derive(Debug, Clone, Default, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
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
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum OffsetReset {
    /// Start from the latest (most recent) offset.
    #[default]
    Latest,
    /// Start from the earliest available offset.
    Earliest,
}

/// Security protocol for Kafka broker connections.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum SecurityProtocol {
    /// Plaintext (no encryption).
    Plaintext,
    /// SSL/TLS encryption.
    #[default]
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

pub(crate) fn default_kafka_base() -> BrokerConfig {
    let mut base = BrokerConfig::new(BACKEND_NAME);
    base.commit_strategy = CommitStrategy::Auto;
    base.dlq = DlqPolicy {
        enabled: false,
        ..DlqPolicy::default()
    };
    base
}

fn apply_adapter_base_defaults(base: &mut BrokerConfig) {
    // Only apply adapter defaults for the backend name; leave all other fields
    // as-is so that explicit user configuration can be validated and rejected
    // by validate() if unsupported.
    if base.backend.is_empty() {
        base.backend = BACKEND_NAME.to_string();
    }
}

pub(crate) fn validate_topic(field: &str, value: &str) -> AppResult<()> {
    if value.trim().is_empty() {
        return invalid(format!("{field} must not be empty"));
    }
    if value.len() > 249 {
        return invalid(format!("{field} must be at most 249 bytes"));
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ':'))
    {
        return invalid(format!(
            "{field} must contain only letters, digits, ., _, -, or :"
        ));
    }
    Ok(())
}

fn redacted_option(value: Option<&String>) -> Option<&'static str> {
    value.as_ref().map(|_| "<redacted>")
}

fn has_url_credentials(value: &str) -> bool {
    let Some(scheme_end) = value.find("://") else {
        return false;
    };
    let authority_start = scheme_end + 3;
    let authority_end = value[authority_start..]
        .find(['/', '?', '#'])
        .map_or(value.len(), |offset| authority_start + offset);
    value[authority_start..authority_end].contains('@')
}

fn redact_uri_credentials(value: &str) -> String {
    let Some(scheme_end) = value.find("://") else {
        return value.to_string();
    };
    let authority_start = scheme_end + 3;
    let authority_end = value[authority_start..]
        .find(['/', '?', '#'])
        .map_or(value.len(), |offset| authority_start + offset);
    let authority = &value[authority_start..authority_end];
    let Some(at_pos) = authority.rfind('@') else {
        return value.to_string();
    };

    format!(
        "{}<redacted>@{}{}",
        &value[..authority_start],
        &authority[at_pos + 1..],
        &value[authority_end..]
    )
}

const fn default_session_timeout() -> Duration {
    Duration::from_secs(30)
}

fn default_brokers() -> Vec<String> {
    vec!["localhost:9092".to_string()]
}

const fn default_batch_size() -> usize {
    1000
}

const fn default_linger_ms() -> u64 {
    5
}

mod duration_seconds {
    use std::time::Duration;

    use serde::{Deserialize, Deserializer};

    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(secs))
    }
}

fn validate_backend(backend: &str) -> AppResult<()> {
    if backend == BACKEND_NAME {
        return Ok(());
    }
    invalid(format!("Kafka config backend must be '{BACKEND_NAME}'"))
}

fn invalid(message: impl Into<String>) -> AppResult<()> {
    Err(AppError::new(ErrorCode::InvalidInput, message.into()))
}
