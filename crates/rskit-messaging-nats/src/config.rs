use std::fmt;

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_messaging::{
    BrokerConfig, BrokerConfigExt, CommitStrategy, DeliveryGuarantee, DlqPolicy,
};
use serde::{Deserialize, Deserializer};

pub(crate) const BACKEND_NAME: &str = "nats";
const DEFAULT_URL: &str = "tls://127.0.0.1:4222";

/// Configuration for the NATS messaging adapter.
///
/// Broker-agnostic fields live in [`BrokerConfig`]. NATS queue groups are configured via
/// [`BrokerConfig::consumer_group`]; adapter fields are limited to NATS client/protocol knobs.
#[derive(Clone)]
pub struct NatsConfig {
    /// Shared broker settings (backend/name/enabled, delivery, retry, DLQ, etc.).
    pub base: BrokerConfig,
    /// NATS server URLs. Credentials and query strings are rejected.
    pub servers: Vec<String>,
    /// Optional prefix added to every published/subscribed subject.
    pub subject_prefix: String,
    /// Authentication token.
    pub token: Option<String>,
    /// Authentication username.
    pub username: Option<String>,
    /// Authentication password.
    pub password: Option<String>,
    /// TCP connection timeout in milliseconds.
    pub connection_timeout: u64,
    /// Maximum reconnect attempts (`None` means unlimited).
    pub max_reconnects: Option<usize>,
    /// Delay between reconnect attempts in milliseconds.
    pub reconnect_delay: u64,
    /// Maximum messages buffered from subscription tasks before applying backpressure.
    pub subscription_buffer: usize,
    /// Permit plaintext connections for explicit local-development use only.
    pub allow_insecure_dev: bool,
}

#[derive(Deserialize)]
struct NatsConfigSerde {
    #[serde(default, flatten)]
    base: BrokerConfig,
    #[serde(default = "default_servers")]
    servers: Vec<String>,
    #[serde(default)]
    subject_prefix: String,
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    password: Option<String>,
    #[serde(default = "default_connection_timeout")]
    connection_timeout: u64,
    #[serde(default)]
    max_reconnects: Option<usize>,
    #[serde(default = "default_reconnect_delay")]
    reconnect_delay: u64,
    #[serde(default = "default_subscription_buffer")]
    subscription_buffer: usize,
    #[serde(default)]
    allow_insecure_dev: bool,
}

impl<'de> Deserialize<'de> for NatsConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut config = NatsConfigSerde::deserialize(deserializer)?;
        apply_adapter_base_defaults(&mut config.base);
        Ok(Self {
            base: config.base,
            servers: config.servers,
            subject_prefix: config.subject_prefix,
            token: config.token,
            username: config.username,
            password: config.password,
            connection_timeout: config.connection_timeout,
            max_reconnects: config.max_reconnects,
            reconnect_delay: config.reconnect_delay,
            subscription_buffer: config.subscription_buffer,
            allow_insecure_dev: config.allow_insecure_dev,
        })
    }
}

impl fmt::Debug for NatsConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NatsConfig")
            .field("backend", &self.base.backend)
            .field("name", &self.base.name)
            .field("enabled", &self.base.enabled)
            .field("servers", &redact_many(&self.servers))
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
            .field("subject_prefix", &self.subject_prefix)
            .field("token", &redacted_option(self.token.as_ref()))
            .field("username", &redacted_option(self.username.as_ref()))
            .field("password", &redacted_option(self.password.as_ref()))
            .field("connection_timeout", &self.connection_timeout)
            .field("max_reconnects", &self.max_reconnects)
            .field("reconnect_delay", &self.reconnect_delay)
            .field("subscription_buffer", &self.subscription_buffer)
            .field("allow_insecure_dev", &self.allow_insecure_dev)
            .finish()
    }
}

impl Default for NatsConfig {
    fn default() -> Self {
        Self {
            base: default_nats_base(),
            servers: default_servers(),
            subject_prefix: String::new(),
            token: None,
            username: None,
            password: None,
            connection_timeout: default_connection_timeout(),
            max_reconnects: None,
            reconnect_delay: default_reconnect_delay(),
            subscription_buffer: default_subscription_buffer(),
            allow_insecure_dev: false,
        }
    }
}

impl BrokerConfigExt for NatsConfig {
    fn base(&self) -> &BrokerConfig {
        &self.base
    }

    fn validate(&self) -> AppResult<()> {
        self.base.validate()?;
        validate_backend(&self.base.backend)?;

        if !matches!(self.base.delivery_guarantee, DeliveryGuarantee::AtMostOnce) {
            return invalid(
                "NATS core adapter supports only at_most_once delivery; use a JetStream adapter for durable acks",
            );
        }
        if !matches!(self.base.commit_strategy, CommitStrategy::Auto) {
            return invalid("NATS core adapter requires commit_strategy=auto");
        }
        if self.base.retries != 0 {
            return invalid(
                "NATS core adapter does not retry publishes; set retries=0 or wrap with resilience middleware",
            );
        }
        if self.base.dlq.enabled {
            return invalid(
                "NATS core adapter does not implement DLQ routing; disable base.dlq.enabled or add DLQ middleware",
            );
        }
        if self.servers.is_empty() {
            return invalid("NATS servers list cannot be empty");
        }
        if self.servers.iter().any(|server| server.trim().is_empty()) {
            return invalid("NATS servers must not contain empty entries");
        }
        for server in &self.servers {
            validate_server(server, self.allow_insecure_dev)?;
        }
        if !self.subject_prefix.is_empty() {
            validate_subject(
                "NATS subject_prefix",
                self.subject_prefix.trim_end_matches('.'),
            )?;
        }
        if let Some(queue_group) = self.base.consumer_group.as_ref() {
            validate_subject("NATS consumer_group", queue_group)?;
        }
        for subject in self
            .base
            .topics
            .iter()
            .chain(self.base.subscriptions.iter())
        {
            subject_for(self, subject)?;
        }
        if self.token.is_some() && (self.username.is_some() || self.password.is_some()) {
            return invalid("NATS token auth and username/password auth are mutually exclusive");
        }
        if self.username.is_some() != self.password.is_some() {
            return invalid("NATS username and password must be configured together");
        }
        if self.connection_timeout == 0 {
            return invalid("NATS connection_timeout must be greater than zero");
        }
        if self.reconnect_delay == 0 {
            return invalid("NATS reconnect_delay must be greater than zero");
        }
        if self.subscription_buffer == 0 {
            return invalid("NATS subscription_buffer must be greater than zero");
        }

        Ok(())
    }
}

pub(crate) fn default_nats_base() -> BrokerConfig {
    let mut base = BrokerConfig::new(BACKEND_NAME);
    base.delivery_guarantee = DeliveryGuarantee::AtMostOnce;
    base.commit_strategy = CommitStrategy::Auto;
    base.retries = 0;
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

fn redacted_option(value: Option<&String>) -> Option<&'static str> {
    value.as_ref().map(|_| "<redacted>")
}

fn redact_many(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(|value| redact_uri_credentials(value))
        .collect()
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

fn default_servers() -> Vec<String> {
    vec![DEFAULT_URL.to_string()]
}

const fn default_connection_timeout() -> u64 {
    5_000
}

const fn default_reconnect_delay() -> u64 {
    100
}

const fn default_subscription_buffer() -> usize {
    1024
}

pub(crate) fn validate_subject(field: &str, value: &str) -> AppResult<()> {
    if value.trim().is_empty() {
        return invalid(format!("{field} must not be empty"));
    }
    if value.len() > 249 {
        return invalid(format!("{field} must be at most 249 bytes"));
    }
    if value.contains("..")
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ':'))
    {
        return invalid(format!(
            "{field} must contain only letters, digits, ., _, -, or : without empty tokens"
        ));
    }
    Ok(())
}

fn validate_server(server: &str, allow_insecure_dev: bool) -> AppResult<()> {
    if has_url_credentials(server) || server.contains('?') {
        return invalid("NATS server URLs must not contain credentials or query strings");
    }
    if !allow_insecure_dev && !server.starts_with("tls://") && !server.starts_with("wss://") {
        return invalid("NATS plaintext URLs require allow_insecure_dev=true");
    }
    Ok(())
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

pub(crate) fn subject_for(config: &NatsConfig, subject: &str) -> AppResult<String> {
    validate_subject("NATS subject", subject)?;
    let combined = if config.subject_prefix.is_empty() {
        subject.to_string()
    } else {
        format!("{}{}", config.subject_prefix, subject)
    };
    validate_subject("NATS combined subject", &combined)?;
    Ok(combined)
}

fn validate_backend(backend: &str) -> AppResult<()> {
    if backend == BACKEND_NAME {
        return Ok(());
    }
    invalid(format!("NATS config backend must be '{BACKEND_NAME}'"))
}

fn invalid(message: impl Into<String>) -> AppResult<()> {
    Err(AppError::new(ErrorCode::InvalidInput, message.into()))
}
