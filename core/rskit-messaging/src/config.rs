use std::time::Duration;

use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::Deserialize;

// ── Base broker configuration ────────────────────────────────────────────────

/// Configuration shared by all message-broker adapters.
///
/// Concrete broker configs embed this struct
/// so generic code can work through [`BrokerConfigExt`] without knowing adapter-specific fields.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct BrokerConfig {
    /// Adapter name used for registry selection.
    #[serde(default = "default_adapter")]
    pub adapter: String,
    /// Logical name for this configuration.
    #[serde(default = "default_name")]
    pub name: String,
    /// Whether this configuration is enabled.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Number of retries for failed requests.
    #[serde(default = "default_retries")]
    pub retries: u32,
    /// Backoff between retry attempts in milliseconds.
    #[serde(default = "default_retry_backoff")]
    pub retry_backoff: u64,
    /// Request timeout in milliseconds (`None` = use broker default).
    #[serde(default)]
    pub request_timeout: Option<u64>,
    /// Requested delivery semantics.
    #[serde(default)]
    pub delivery_guarantee: DeliveryGuarantee,
    /// Offset/ack commit behavior.
    #[serde(default)]
    pub commit_strategy: CommitStrategy,
    /// Dead-letter queue policy.
    #[serde(default)]
    pub dlq: DlqPolicy,
    /// Maximum number of in-flight messages.
    #[serde(default = "default_max_in_flight")]
    pub max_in_flight: usize,
    /// Broker-neutral consumer group name.
    #[serde(default)]
    pub consumer_group: Option<String>,
    /// Broker-neutral topic declarations.
    #[serde(default)]
    pub topics: Vec<String>,
    /// Broker-neutral subscription subjects/topics. Empty means the consumer chooses at runtime.
    #[serde(default)]
    pub subscriptions: Vec<String>,
}

impl BrokerConfig {
    /// Create a broker config with the provided adapter name and shared defaults.
    #[must_use]
    pub fn new(adapter: impl Into<String>) -> Self {
        Self {
            adapter: adapter.into(),
            ..Self::default()
        }
    }

    /// Return the request timeout as a [`Duration`], if configured.
    #[must_use]
    pub fn request_timeout_duration(&self) -> Option<Duration> {
        self.request_timeout.map(Duration::from_millis)
    }

    /// Return retry backoff as a [`Duration`].
    #[must_use]
    pub const fn retry_backoff_duration(&self) -> Duration {
        Duration::from_millis(self.retry_backoff)
    }

    /// Validate shared broker-neutral configuration.
    pub fn validate(&self) -> AppResult<()> {
        validate_name("messaging adapter", &self.adapter)?;
        validate_name("messaging config name", &self.name)?;

        if self.max_in_flight == 0 {
            return invalid("messaging max_in_flight must be greater than zero");
        }

        if let Some(timeout) = self.request_timeout
            && timeout == 0
        {
            return invalid("messaging request_timeout must be greater than zero when set");
        }

        if self.retries > 0 && self.retry_backoff == 0 {
            return invalid(
                "messaging retry_backoff must be greater than zero when retries are enabled",
            );
        }

        validate_optional_name("messaging consumer_group", self.consumer_group.as_deref())?;

        for topic in &self.topics {
            validate_topic_like("messaging topic", topic)?;
        }
        for subscription in &self.subscriptions {
            validate_topic_like("messaging subscription", subscription)?;
        }

        self.dlq.validate()
    }
}

impl Default for BrokerConfig {
    fn default() -> Self {
        Self {
            adapter: default_adapter(),
            name: default_name(),
            enabled: default_enabled(),
            retries: default_retries(),
            retry_backoff: default_retry_backoff(),
            request_timeout: None,
            delivery_guarantee: DeliveryGuarantee::default(),
            commit_strategy: CommitStrategy::default(),
            dlq: DlqPolicy::default(),
            max_in_flight: default_max_in_flight(),
            consumer_group: None,
            topics: Vec::new(),
            subscriptions: Vec::new(),
        }
    }
}

/// Partial broker config used by adapter crates to apply only user-provided shared settings over adapter-specific defaults.
#[doc(hidden)]
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct BrokerConfigOverrides {
    /// Adapter name used for registry selection.
    pub adapter: Option<String>,
    /// Logical name for this configuration.
    pub name: Option<String>,
    /// Whether this configuration is enabled.
    pub enabled: Option<bool>,
    /// Number of retries for failed requests.
    pub retries: Option<u32>,
    /// Backoff between retry attempts in milliseconds.
    pub retry_backoff: Option<u64>,
    /// Request timeout in milliseconds.
    pub request_timeout: Option<Option<u64>>,
    /// Requested delivery semantics.
    pub delivery_guarantee: Option<DeliveryGuarantee>,
    /// Offset/ack commit behavior.
    pub commit_strategy: Option<CommitStrategy>,
    /// Dead-letter queue policy.
    pub dlq: Option<DlqPolicy>,
    /// Maximum number of in-flight messages.
    pub max_in_flight: Option<usize>,
    /// Broker-neutral consumer group name.
    pub consumer_group: Option<Option<String>>,
    /// Broker-neutral topic declarations.
    pub topics: Option<Vec<String>>,
    /// Broker-neutral subscription subjects/topics.
    pub subscriptions: Option<Vec<String>>,
}

impl BrokerConfigOverrides {
    /// Apply explicitly provided fields to `base`.
    pub fn apply_to(self, base: &mut BrokerConfig) {
        if let Some(value) = self.adapter {
            base.adapter = value;
        }
        if let Some(value) = self.name {
            base.name = value;
        }
        if let Some(value) = self.enabled {
            base.enabled = value;
        }
        if let Some(value) = self.retries {
            base.retries = value;
        }
        if let Some(value) = self.retry_backoff {
            base.retry_backoff = value;
        }
        if let Some(value) = self.request_timeout {
            base.request_timeout = value;
        }
        if let Some(value) = self.delivery_guarantee {
            base.delivery_guarantee = value;
        }
        if let Some(value) = self.commit_strategy {
            base.commit_strategy = value;
        }
        if let Some(value) = self.dlq {
            base.dlq = value;
        }
        if let Some(value) = self.max_in_flight {
            base.max_in_flight = value;
        }
        if let Some(value) = self.consumer_group {
            base.consumer_group = value;
        }
        if let Some(value) = self.topics {
            base.topics = value;
        }
        if let Some(value) = self.subscriptions {
            base.subscriptions = value;
        }
    }
}

/// Extension trait for broker-specific configurations.
///
/// Every adapter configuration struct should implement this
/// so that generic infrastructure (retry policies, health checks, service discovery) can access the common [`BrokerConfig`]
/// and perform validation without knowing the concrete broker type.
pub trait BrokerConfigExt {
    /// Access the shared broker configuration.
    fn base(&self) -> &BrokerConfig;
    /// Validate the complete configuration (base + adapter-specific fields).
    fn validate(&self) -> AppResult<()>;
}

const fn default_retries() -> u32 {
    3
}

const fn default_retry_backoff() -> u64 {
    100
}

fn default_adapter() -> String {
    "memory".to_string()
}

const fn default_max_in_flight() -> usize {
    1
}

fn default_name() -> String {
    "default".to_string()
}

/// Requested broker delivery semantics.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum DeliveryGuarantee {
    /// Message loss is allowed but redelivery is avoided.
    AtMostOnce,
    /// Default: messages may redeliver after failure.
    #[default]
    AtLeastOnce,
    /// Broker-supported exactly-once semantics.
    ExactlyOnce,
}

/// Offset/ack commit behavior.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum CommitStrategy {
    /// Delegate commit behavior to the broker client.
    Auto,
    /// Default: commit after handler success.
    #[default]
    PostHandlerSuccess,
    /// Application code commits manually.
    Manual,
}

/// Dead-letter queue policy.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct DlqPolicy {
    /// Whether DLQ routing is enabled.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Suffix appended to source topic when no explicit DLQ topic is configured.
    #[serde(default = "default_dlq_suffix")]
    pub suffix: String,
}

impl Default for DlqPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            suffix: default_dlq_suffix(),
        }
    }
}

impl DlqPolicy {
    /// Validate DLQ policy fields.
    pub fn validate(&self) -> AppResult<()> {
        if !self.enabled {
            return Ok(());
        }

        if self.suffix.is_empty() {
            return invalid("messaging dlq suffix is required when DLQ is enabled");
        }
        if self.suffix.len() > 64 {
            return invalid("messaging dlq suffix must be at most 64 bytes");
        }
        if self.suffix.chars().any(char::is_whitespace) {
            return invalid("messaging dlq suffix must not contain whitespace");
        }
        if self.suffix.contains('/') || self.suffix.contains('\\') {
            return invalid("messaging dlq suffix must not contain path separators");
        }

        Ok(())
    }
}

const fn default_enabled() -> bool {
    true
}

fn default_dlq_suffix() -> String {
    ".dlq".to_string()
}

fn validate_optional_name(field: &str, value: Option<&str>) -> AppResult<()> {
    if let Some(value) = value {
        validate_topic_like(field, value)?;
    }
    Ok(())
}

fn validate_topic_like(field: &str, value: &str) -> AppResult<()> {
    if value.trim().is_empty() {
        return invalid(format!("{field} is required"));
    }
    if value
        .chars()
        .any(|ch| ch.is_control() || ch.is_whitespace())
    {
        return invalid(format!(
            "{field} must not contain whitespace or control characters"
        ));
    }
    Ok(())
}

fn validate_name(field: &str, value: &str) -> AppResult<()> {
    if value.trim().is_empty() {
        return invalid(format!("{field} is required"));
    }
    if value.len() > 128 {
        return invalid(format!("{field} must be at most 128 bytes"));
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return invalid(format!(
            "{field} must contain only letters, digits, ., _, or -"
        ));
    }
    Ok(())
}

fn invalid(message: impl Into<String>) -> AppResult<()> {
    Err(AppError::new(ErrorCode::InvalidInput, message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broker_config_defaults_are_broker_neutral() {
        let config = BrokerConfig::default();

        assert_eq!(config.adapter, "memory");
        assert_eq!(config.name, "default");
        assert!(config.enabled);
        assert_eq!(config.retries, 3);
        assert_eq!(config.retry_backoff_duration(), Duration::from_millis(100));
        assert!(config.request_timeout_duration().is_none());
        assert_eq!(config.delivery_guarantee, DeliveryGuarantee::AtLeastOnce);
        assert_eq!(config.commit_strategy, CommitStrategy::PostHandlerSuccess);
        assert_eq!(config.max_in_flight, 1);
        assert!(config.consumer_group.is_none());
        assert!(config.topics.is_empty());
        assert!(config.subscriptions.is_empty());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn broker_config_validation_rejects_invalid_shared_values() {
        let config = BrokerConfig {
            max_in_flight: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        let config = BrokerConfig {
            request_timeout: Some(0),
            ..Default::default()
        };
        assert!(config.validate().is_err());

        let config = BrokerConfig {
            retry_backoff: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        let mut config = BrokerConfig::default();
        config.dlq.suffix = "bad suffix".to_string();
        assert!(config.validate().is_err());
    }
}
