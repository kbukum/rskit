#![expect(
    clippy::redundant_pub_crate,
    reason = "config helpers are shared with the crate root but remain crate-internal"
)]

use std::fmt;

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_messaging::{
    BrokerConfig, BrokerConfigExt, BrokerConfigOverrides, CommitStrategy, DeliveryGuarantee,
    DlqPolicy,
};
use serde::{Deserialize, Deserializer};

pub(crate) const ADAPTER_NAME: &str = "rabbitmq";
const DEFAULT_URI: &str = "amqps://127.0.0.1:5671/%2f";

/// Configuration for the `RabbitMQ` messaging adapter.
///
/// Broker-agnostic fields live in [`BrokerConfig`]. Adapter fields are limited to AMQP
/// connection, routing, queue declaration, acknowledgement, and prefetch knobs.
#[derive(Clone)]
pub struct RabbitMqConfig {
    /// Shared broker settings (adapter/name/enabled, delivery, retry, DLQ, etc.).
    pub base: BrokerConfig,
    /// AMQP connection URI. Credentials and query strings are rejected.
    pub uri: String,
    /// Exchange used for publish routing; empty string uses `RabbitMQ`'s default direct exchange.
    pub exchange: String,
    /// Prefix added to queue/routing-key names.
    pub queue_prefix: String,
    /// Declare queues before publishing/subscribing.
    pub declare_queues: bool,
    /// Mark declared queues durable.
    pub durable_queues: bool,
    /// Override `RabbitMQ` `no_ack`; `None` derives from [`BrokerConfig::commit_strategy`].
    pub auto_ack: Option<bool>,
    /// Consumer tag used for subscriptions.
    pub consumer_tag: String,
    /// AMQP prefetch count; `None` uses shared `max_in_flight`.
    pub prefetch_count: Option<u16>,
    /// Connection timeout in milliseconds.
    pub connection_timeout: u64,
    /// Maximum messages buffered from consumer tasks before applying backpressure.
    pub subscription_buffer: usize,
    /// Permit plaintext connections for explicit local-development use only.
    pub allow_insecure_dev: bool,
}

#[derive(Deserialize)]
struct RabbitMqConfigSerde {
    #[serde(default, flatten)]
    base: BrokerConfigOverrides,
    #[serde(default = "default_uri")]
    uri: String,
    #[serde(default)]
    exchange: String,
    #[serde(default)]
    queue_prefix: String,
    #[serde(default = "default_true")]
    declare_queues: bool,
    #[serde(default = "default_true")]
    durable_queues: bool,
    #[serde(default)]
    auto_ack: Option<bool>,
    #[serde(default = "default_consumer_tag")]
    consumer_tag: String,
    #[serde(default)]
    prefetch_count: Option<u16>,
    #[serde(default = "default_connection_timeout")]
    connection_timeout: u64,
    #[serde(default = "default_subscription_buffer")]
    subscription_buffer: usize,
    #[serde(default)]
    allow_insecure_dev: bool,
}

impl<'de> Deserialize<'de> for RabbitMqConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let config = RabbitMqConfigSerde::deserialize(deserializer)?;
        let mut base = default_rabbitmq_base();
        config.base.apply_to(&mut base);
        Ok(Self {
            base,
            uri: config.uri,
            exchange: config.exchange,
            queue_prefix: config.queue_prefix,
            declare_queues: config.declare_queues,
            durable_queues: config.durable_queues,
            auto_ack: config.auto_ack,
            consumer_tag: config.consumer_tag,
            prefetch_count: config.prefetch_count,
            connection_timeout: config.connection_timeout,
            subscription_buffer: config.subscription_buffer,
            allow_insecure_dev: config.allow_insecure_dev,
        })
    }
}

impl fmt::Debug for RabbitMqConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RabbitMqConfig")
            .field("adapter", &self.base.adapter)
            .field("name", &self.base.name)
            .field("enabled", &self.base.enabled)
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
            .field("uri", &redact_uri_credentials(&self.uri))
            .field("exchange", &self.exchange)
            .field("queue_prefix", &self.queue_prefix)
            .field("declare_queues", &self.declare_queues)
            .field("durable_queues", &self.durable_queues)
            .field("auto_ack", &self.auto_ack)
            .field("effective_auto_ack", &self.effective_auto_ack())
            .field("consumer_tag", &self.consumer_tag)
            .field("prefetch_count", &self.prefetch_count)
            .field("connection_timeout", &self.connection_timeout)
            .field("subscription_buffer", &self.subscription_buffer)
            .field("allow_insecure_dev", &self.allow_insecure_dev)
            .finish()
    }
}

impl Default for RabbitMqConfig {
    fn default() -> Self {
        Self {
            base: default_rabbitmq_base(),
            uri: default_uri(),
            exchange: String::new(),
            queue_prefix: String::new(),
            declare_queues: true,
            durable_queues: true,
            auto_ack: None,
            consumer_tag: default_consumer_tag(),
            prefetch_count: None,
            connection_timeout: default_connection_timeout(),
            subscription_buffer: default_subscription_buffer(),
            allow_insecure_dev: false,
        }
    }
}

impl RabbitMqConfig {
    /// Return the effective `no_ack` value derived from explicit config or commit strategy.
    #[must_use]
    pub fn effective_auto_ack(&self) -> bool {
        self.auto_ack
            .unwrap_or(matches!(self.base.commit_strategy, CommitStrategy::Auto))
    }

    /// Return the effective AMQP prefetch count.
    pub fn effective_prefetch_count(&self) -> AppResult<u16> {
        if let Some(prefetch_count) = self.prefetch_count {
            return Ok(prefetch_count);
        }
        u16::try_from(self.base.max_in_flight).map_err(|_| {
            AppError::new(
                ErrorCode::InvalidInput,
                "RabbitMQ max_in_flight must fit in AMQP prefetch_count",
            )
        })
    }
}

impl BrokerConfigExt for RabbitMqConfig {
    fn base(&self) -> &BrokerConfig {
        &self.base
    }

    fn validate(&self) -> AppResult<()> {
        self.base.validate()?;
        validate_adapter(&self.base.adapter)?;

        if matches!(self.base.delivery_guarantee, DeliveryGuarantee::ExactlyOnce) {
            return invalid("RabbitMQ exactly_once delivery is not supported by this adapter");
        }
        if !matches!(self.base.commit_strategy, CommitStrategy::Auto) {
            return invalid(
                "RabbitMQ MessageConsumer supports only commit_strategy=auto; post_handler_success/manual require an ack-capable consumer API",
            );
        }
        if self.base.retries != 0 {
            return invalid(
                "RabbitMQ adapter does not retry publishes; set retries=0 or wrap with resilience middleware",
            );
        }
        if self.base.dlq.enabled {
            return invalid(
                "RabbitMQ adapter does not implement DLQ routing; disable base.dlq.enabled or add DLQ middleware",
            );
        }
        if self.base.request_timeout.is_some() {
            return invalid(
                "RabbitMQ adapter does not use request_timeout; configure connection_timeout instead",
            );
        }
        if self.base.consumer_group.is_some() {
            return invalid(
                "RabbitMQ adapter does not support consumer_group; use distinct queue names/subscriptions",
            );
        }
        for queue in self
            .base
            .topics
            .iter()
            .chain(self.base.subscriptions.iter())
        {
            queue_for(self, queue)?;
        }
        if self.uri.trim().is_empty() {
            return invalid("RabbitMQ uri is required");
        }
        validate_uri(&self.uri, self.allow_insecure_dev)?;
        validate_name("RabbitMQ consumer_tag", &self.consumer_tag)?;
        if !self.exchange.is_empty() {
            validate_name("RabbitMQ exchange", &self.exchange)?;
        }
        if !self.queue_prefix.is_empty() {
            validate_name(
                "RabbitMQ queue_prefix",
                self.queue_prefix.trim_end_matches('.'),
            )?;
        }
        if self.subscription_buffer == 0 {
            return invalid("RabbitMQ subscription_buffer must be greater than zero");
        }
        if self.connection_timeout == 0 {
            return invalid("RabbitMQ connection_timeout must be greater than zero");
        }
        if self.effective_prefetch_count()? == 0 {
            return invalid("RabbitMQ prefetch_count must be greater than zero");
        }
        if !matches!(self.base.delivery_guarantee, DeliveryGuarantee::AtMostOnce) {
            return invalid(
                "RabbitMQ MessageConsumer supports only at_most_once delivery without handler-coupled acknowledgements",
            );
        }
        if !self.effective_auto_ack() {
            return invalid("RabbitMQ auto acknowledgements are required for MessageConsumer");
        }

        Ok(())
    }
}

pub(crate) fn default_rabbitmq_base() -> BrokerConfig {
    let mut base = BrokerConfig::new(ADAPTER_NAME);
    base.delivery_guarantee = DeliveryGuarantee::AtMostOnce;
    base.commit_strategy = CommitStrategy::Auto;
    base.retries = 0;
    base.dlq = DlqPolicy {
        enabled: false,
        ..DlqPolicy::default()
    };
    base
}

fn default_uri() -> String {
    DEFAULT_URI.to_string()
}

const fn default_true() -> bool {
    true
}

fn default_consumer_tag() -> String {
    "rskit-messaging".to_string()
}

const fn default_connection_timeout() -> u64 {
    5_000
}

const fn default_subscription_buffer() -> usize {
    1024
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

pub(crate) fn validate_name(field: &str, value: &str) -> AppResult<()> {
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

fn validate_uri(uri: &str, allow_insecure_dev: bool) -> AppResult<()> {
    if has_url_credentials(uri) || uri.contains('?') {
        return invalid("RabbitMQ URIs must not contain credentials or query strings");
    }
    if !allow_insecure_dev && !uri.starts_with("amqps://") {
        return invalid("RabbitMQ plaintext URIs require allow_insecure_dev=true");
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

pub(crate) fn queue_for(config: &RabbitMqConfig, queue: &str) -> AppResult<String> {
    validate_name("RabbitMQ queue", queue)?;
    let combined = if config.queue_prefix.is_empty() {
        queue.to_string()
    } else {
        format!("{}{}", config.queue_prefix, queue)
    };
    validate_name("RabbitMQ combined queue/routing key", &combined)?;
    Ok(combined)
}

fn validate_adapter(adapter: &str) -> AppResult<()> {
    if adapter == ADAPTER_NAME {
        return Ok(());
    }
    invalid(format!("RabbitMQ config adapter must be '{ADAPTER_NAME}'"))
}

fn invalid(message: impl Into<String>) -> AppResult<()> {
    Err(AppError::new(ErrorCode::InvalidInput, message.into()))
}
