//! Dead-letter queue middleware for failed messages.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rskit_errors::AppResult;
use serde::Serialize;

use crate::handler::{HandlerMiddleware, MessageHandler};
use crate::message::Message;
use crate::traits::MessageProducer;

const REDACTED: &str = "<redacted>";
const MAX_DLQ_PAYLOAD_CHARS: usize = 4096;
const MAX_BINARY_PREVIEW_BYTES: usize = 32;
const SENSITIVE_PARTS: &[&str] = &[
    "authorization",
    "cookie",
    "token",
    "secret",
    "password",
    "credential",
    "api-key",
    "apikey",
];

/// Configuration for the dead-letter middleware.
#[derive(Debug, Clone)]
pub struct DeadLetterConfig {
    /// Suffix appended to the original topic to form the DLQ topic name.
    pub suffix: String,
}

impl Default for DeadLetterConfig {
    fn default() -> Self {
        Self {
            suffix: ".dlq".to_string(),
        }
    }
}

/// Canonical envelope written to DLQ topics.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeadLetterEnvelope<T> {
    /// Original source topic.
    pub original_topic: String,
    /// Redacted terminal error summary.
    pub error: String,
    /// Number of retries attempted before DLQ routing.
    pub retry_count: u32,
    /// UTC timestamp when the DLQ envelope was created.
    pub timestamp: DateTime<Utc>,
    /// Redacted message headers/metadata.
    pub headers: HashMap<String, String>,
    /// Original payload for typed in-process use. Serialized DLQ envelopes omit
    /// this field so JSON adapters do not leak raw failed-message contents.
    #[serde(skip)]
    pub payload: T,
    /// Redacted string summary for logs, JSON adapters, and non-text payloads.
    pub payload_summary: String,
}

/// Payload types that can provide a safe dead-letter summary without requiring text display.
pub trait DeadLetterPayloadSummary: Send + Sync {
    /// Return a redacted, bounded payload summary for DLQ metadata.
    fn dead_letter_payload_summary(&self) -> String;
}

impl DeadLetterPayloadSummary for String {
    fn dead_letter_payload_summary(&self) -> String {
        sanitize_summary(self)
    }
}

impl DeadLetterPayloadSummary for Vec<u8> {
    fn dead_letter_payload_summary(&self) -> String {
        binary_payload_summary(self)
    }
}

/// Create a dead-letter middleware that routes terminal failures to a DLQ.
///
/// When the inner handler returns an error, a [`DeadLetterEnvelope`] is sent to
/// `<original_topic><suffix>`. A successful DLQ publish swallows the terminal
/// handler error so poison-pill messages do not stall loops. DLQ publish
/// failures are propagated.
pub fn dead_letter<T: Send + Sync + Clone + DeadLetterPayloadSummary + 'static>(
    producer: Arc<dyn MessageProducer<DeadLetterEnvelope<T>>>,
    config: DeadLetterConfig,
) -> impl HandlerMiddleware<T> {
    DeadLetterMiddleware { producer, config }
}

struct DeadLetterMiddleware<T: Send + Sync + 'static> {
    producer: Arc<dyn MessageProducer<DeadLetterEnvelope<T>>>,
    config: DeadLetterConfig,
}

impl<T: Send + Sync + Clone + DeadLetterPayloadSummary + 'static> HandlerMiddleware<T>
    for DeadLetterMiddleware<T>
{
    fn wrap(&self, next: Arc<dyn MessageHandler<T>>) -> Arc<dyn MessageHandler<T>> {
        Arc::new(DeadLetterHandler {
            producer: self.producer.clone(),
            suffix: self.config.suffix.clone(),
            next,
        })
    }
}

struct DeadLetterHandler<T: Send + Sync + 'static> {
    producer: Arc<dyn MessageProducer<DeadLetterEnvelope<T>>>,
    suffix: String,
    next: Arc<dyn MessageHandler<T>>,
}

#[async_trait]
impl<T: Send + Sync + Clone + DeadLetterPayloadSummary + 'static> MessageHandler<T>
    for DeadLetterHandler<T>
{
    async fn handle(&self, msg: Message<T>) -> AppResult<()> {
        let backup = msg.clone();
        match self.next.handle(msg).await {
            Ok(()) => Ok(()),
            Err(e) => {
                let dlq_topic = format!("{}{}", backup.topic, self.suffix);
                let envelope = DeadLetterEnvelope {
                    original_topic: backup.topic.clone(),
                    error: sanitize_summary(&e.to_string()),
                    retry_count: retry_count(&backup.headers),
                    timestamp: Utc::now(),
                    headers: redact_headers(&backup.headers),
                    payload_summary: backup.payload.dead_letter_payload_summary(),
                    payload: backup.payload,
                };
                let mut dlq_msg = Message::new(dlq_topic, envelope);
                dlq_msg.key = backup.key.or_else(|| Some("dlq".to_string()));
                self.producer.send(dlq_msg).await
            }
        }
    }
}

fn retry_count(headers: &HashMap<String, String>) -> u32 {
    headers
        .get("x-retry-count")
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0)
}

fn redact_headers(headers: &HashMap<String, String>) -> HashMap<String, String> {
    headers
        .iter()
        .map(|(key, value)| {
            if is_sensitive(key) || is_sensitive(value) {
                (key.clone(), REDACTED.to_string())
            } else {
                (key.clone(), value.clone())
            }
        })
        .collect()
}

fn sanitize_summary(value: &str) -> String {
    if is_sensitive(value) {
        REDACTED.to_string()
    } else {
        truncate(value)
    }
}

fn binary_payload_summary(value: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut preview = String::new();
    for byte in value.iter().take(MAX_BINARY_PREVIEW_BYTES) {
        let _ = write!(preview, "{byte:02x}");
    }
    if value.len() > MAX_BINARY_PREVIEW_BYTES {
        format!(
            "binary payload: {} bytes, hex preview: {preview}…",
            value.len()
        )
    } else {
        format!("binary payload: {} bytes, hex: {preview}", value.len())
    }
}

fn truncate(value: &str) -> String {
    if value.chars().count() <= MAX_DLQ_PAYLOAD_CHARS {
        return value.to_string();
    }
    value
        .chars()
        .take(MAX_DLQ_PAYLOAD_CHARS)
        .collect::<String>()
        + "…"
}

fn is_sensitive(value: &str) -> bool {
    let lowered = value.to_ascii_lowercase();
    SENSITIVE_PARTS.iter().any(|part| lowered.contains(part))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::handler::{FnHandler, chain_handlers};
    use crate::memory::InMemoryBroker;
    use crate::traits::MessageConsumer;
    use rskit_errors::{AppError, ErrorCode};

    #[tokio::test]
    async fn success_does_not_produce_dlq() {
        let broker = InMemoryBroker::<DeadLetterEnvelope<String>>::new(16);
        let dlq_producer = broker.producer();
        let dlq_consumer = broker.consumer();
        dlq_consumer.subscribe(&["topic.dlq"]).await.unwrap();

        let base: Arc<dyn MessageHandler<String>> =
            Arc::new(FnHandler::new(|_msg: Message<String>| async { Ok(()) }));

        let mw: Arc<dyn HandlerMiddleware<String>> = Arc::new(DeadLetterMiddleware {
            producer: Arc::new(dlq_producer),
            config: DeadLetterConfig::default(),
        });
        let handler = chain_handlers(base, &[mw]);

        handler
            .handle(Message::new("topic", "ok".to_string()))
            .await
            .unwrap();

        let result = dlq_consumer.recv(Duration::from_millis(50)).await;
        assert_eq!(
            result.unwrap_err().code(),
            ErrorCode::Timeout,
            "should not have received a DLQ message"
        );
    }

    #[tokio::test]
    async fn failure_routes_to_dlq_and_swallows_terminal_error() {
        let broker = InMemoryBroker::<DeadLetterEnvelope<String>>::new(16);
        let dlq_producer = broker.producer();
        let dlq_consumer = broker.consumer();
        dlq_consumer.subscribe(&["topic.dlq"]).await.unwrap();

        let base: Arc<dyn MessageHandler<String>> =
            Arc::new(FnHandler::new(|_msg: Message<String>| async {
                Err(AppError::new(ErrorCode::Internal, "boom"))
            }));

        let mw: Arc<dyn HandlerMiddleware<String>> = Arc::new(DeadLetterMiddleware {
            producer: Arc::new(dlq_producer),
            config: DeadLetterConfig::default(),
        });
        let handler = chain_handlers(base, &[mw]);

        handler
            .handle(Message::new("topic", "fail".to_string()))
            .await
            .unwrap();

        let dlq_msg = tokio::time::timeout(
            Duration::from_millis(200),
            dlq_consumer.recv(std::time::Duration::from_millis(50)),
        )
        .await
        .expect("should receive DLQ message")
        .unwrap();
        assert_eq!(dlq_msg.topic, "topic.dlq");
        assert_eq!(dlq_msg.payload.original_topic, "topic");
        assert_eq!(dlq_msg.payload.error, "INTERNAL_ERROR: boom");
        assert_eq!(dlq_msg.payload.payload, "fail");
    }

    #[tokio::test]
    async fn custom_suffix_retry_count_and_redaction_are_recorded() {
        let broker = InMemoryBroker::<DeadLetterEnvelope<String>>::new(16);
        let dlq_producer = broker.producer();
        let dlq_consumer = broker.consumer();
        dlq_consumer.subscribe(&["orders.dead"]).await.unwrap();

        let base: Arc<dyn MessageHandler<String>> =
            Arc::new(FnHandler::new(|_msg: Message<String>| async {
                Err(AppError::new(ErrorCode::Internal, "token leaked"))
            }));
        let mw: Arc<dyn HandlerMiddleware<String>> = Arc::new(DeadLetterMiddleware {
            producer: Arc::new(dlq_producer),
            config: DeadLetterConfig {
                suffix: ".dead".to_string(),
            },
        });
        let handler = chain_handlers(base, &[mw]);

        handler
            .handle(
                Message::new("orders", "password=secret".to_string())
                    .with_header("x-retry-count", "2")
                    .with_header("authorization", "Bearer secret")
                    .with_header("trace-id", "abc"),
            )
            .await
            .unwrap();

        let dlq_msg = tokio::time::timeout(
            Duration::from_millis(200),
            dlq_consumer.recv(std::time::Duration::from_millis(50)),
        )
        .await
        .expect("should receive DLQ message")
        .unwrap();
        let envelope = dlq_msg.payload;
        assert_eq!(dlq_msg.topic, "orders.dead");
        assert_eq!(envelope.retry_count, 2);
        assert_eq!(envelope.error, REDACTED);
        assert_eq!(envelope.payload_summary, REDACTED);
        assert_eq!(envelope.headers["authorization"], REDACTED);
        assert_eq!(envelope.headers["trace-id"], "abc");
    }

    #[tokio::test]
    async fn dlq_publish_failure_propagates() {
        let broker = InMemoryBroker::<DeadLetterEnvelope<String>>::new(16);
        let dlq_producer = broker.producer();
        let base: Arc<dyn MessageHandler<String>> =
            Arc::new(FnHandler::new(|_msg: Message<String>| async {
                Err(AppError::new(ErrorCode::Internal, "boom"))
            }));
        let mw: Arc<dyn HandlerMiddleware<String>> = Arc::new(DeadLetterMiddleware {
            producer: Arc::new(dlq_producer),
            config: DeadLetterConfig::default(),
        });
        let handler = chain_handlers(base, &[mw]);

        let result = handler
            .handle(Message::new("topic", "fail".to_string()))
            .await;
        assert!(result.is_err());
    }
    #[tokio::test]
    async fn binary_payload_routes_to_dlq_without_string_conversion() {
        let broker = InMemoryBroker::<DeadLetterEnvelope<Vec<u8>>>::new(16);
        let dlq_producer = broker.producer();
        let dlq_consumer = broker.consumer();
        dlq_consumer.subscribe(&["topic.dlq"]).await.unwrap();

        let base: Arc<dyn MessageHandler<Vec<u8>>> =
            Arc::new(FnHandler::new(|_msg: Message<Vec<u8>>| async {
                Err(AppError::new(ErrorCode::Internal, "boom"))
            }));

        let mw: Arc<dyn HandlerMiddleware<Vec<u8>>> = Arc::new(DeadLetterMiddleware {
            producer: Arc::new(dlq_producer),
            config: DeadLetterConfig::default(),
        });
        let handler = chain_handlers(base, &[mw]);

        handler
            .handle(Message::new("topic", vec![0, 1, 2, 0xff]))
            .await
            .unwrap();

        let dlq_msg = tokio::time::timeout(
            Duration::from_millis(200),
            dlq_consumer.recv(std::time::Duration::from_millis(50)),
        )
        .await
        .expect("should receive DLQ message")
        .unwrap();
        assert_eq!(dlq_msg.payload.payload, vec![0, 1, 2, 0xff]);
        assert_eq!(
            dlq_msg.payload.payload_summary,
            "binary payload: 4 bytes, hex: 000102ff"
        );
    }
}
