//! Message deduplication middleware based on the `message-id` header.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use rskit_errors::AppResult;

use crate::handler::{HandlerMiddleware, MessageHandler};
use crate::message::Message;

/// Configuration for the deduplication middleware.
#[derive(Debug, Clone)]
pub struct DedupConfig {
    /// Maximum number of message IDs to track.
    pub window_size: usize,
    /// Time-to-live for tracked IDs; entries older than this are purged.
    pub ttl: Duration,
}

impl Default for DedupConfig {
    fn default() -> Self {
        Self {
            window_size: 10_000,
            ttl: Duration::from_secs(300),
        }
    }
}

/// Create a deduplication middleware.
///
/// Messages that carry a `message-id` header are tracked. If a duplicate
/// ID arrives within the configured TTL window it is silently dropped.
/// Messages without a `message-id` header are always forwarded.
pub fn dedup<T: Send + Sync + 'static>(config: DedupConfig) -> impl HandlerMiddleware<T> {
    DedupMiddleware {
        config,
        seen: Arc::new(parking_lot::Mutex::new(HashMap::new())),
    }
}

struct DedupMiddleware {
    config: DedupConfig,
    seen: Arc<parking_lot::Mutex<HashMap<String, Instant>>>,
}

impl<T: Send + Sync + 'static> HandlerMiddleware<T> for DedupMiddleware {
    fn wrap(&self, next: Arc<dyn MessageHandler<T>>) -> Arc<dyn MessageHandler<T>> {
        Arc::new(DedupHandler {
            config: self.config.clone(),
            seen: self.seen.clone(),
            next,
        })
    }
}

struct DedupHandler<T: Send + Sync + 'static> {
    config: DedupConfig,
    seen: Arc<parking_lot::Mutex<HashMap<String, Instant>>>,
    next: Arc<dyn MessageHandler<T>>,
}

#[async_trait]
impl<T: Send + Sync + 'static> MessageHandler<T> for DedupHandler<T> {
    async fn handle(&self, msg: Message<T>) -> AppResult<()> {
        if let Some(id) = msg.headers.get("message-id") {
            let mut seen = self.seen.lock();
            let now = Instant::now();

            // Purge expired entries.
            seen.retain(|_, ts| now.duration_since(*ts) < self.config.ttl);

            if seen.contains_key(id) {
                ::tracing::debug!(message_id = %id, "duplicate message skipped");
                return Ok(());
            }

            // Enforce window size by evicting the oldest entry.
            while seen.len() >= self.config.window_size {
                let oldest = seen
                    .iter()
                    .min_by_key(|(_, ts)| *ts)
                    .map(|(k, _)| k.clone());
                if let Some(key) = oldest {
                    seen.remove(&key);
                } else {
                    break;
                }
            }

            seen.insert(id.clone(), now);
        }
        self.next.handle(msg).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;
    use crate::handler::{FnHandler, chain_handlers};

    fn counting_handler(counter: &Arc<AtomicU32>) -> Arc<dyn MessageHandler<String>> {
        let c = counter.clone();
        Arc::new(FnHandler::new(move |_msg: Message<String>| {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }))
    }

    #[tokio::test]
    async fn duplicate_message_is_skipped() {
        let counter = Arc::new(AtomicU32::new(0));
        let mw = DedupMiddleware {
            config: DedupConfig::default(),
            seen: Arc::new(parking_lot::Mutex::new(HashMap::new())),
        };
        let handler = chain_handlers(
            counting_handler(&counter),
            &[Arc::new(mw) as Arc<dyn HandlerMiddleware<String>>],
        );

        let msg1 = Message::new("t", "a".to_string()).with_header("message-id", "id-1");
        let msg2 = Message::new("t", "b".to_string()).with_header("message-id", "id-1");

        handler.handle(msg1).await.unwrap();
        handler.handle(msg2).await.unwrap();

        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn different_ids_are_processed() {
        let counter = Arc::new(AtomicU32::new(0));
        let mw = DedupMiddleware {
            config: DedupConfig::default(),
            seen: Arc::new(parking_lot::Mutex::new(HashMap::new())),
        };
        let handler = chain_handlers(
            counting_handler(&counter),
            &[Arc::new(mw) as Arc<dyn HandlerMiddleware<String>>],
        );

        let msg1 = Message::new("t", "a".to_string()).with_header("message-id", "id-1");
        let msg2 = Message::new("t", "b".to_string()).with_header("message-id", "id-2");

        handler.handle(msg1).await.unwrap();
        handler.handle(msg2).await.unwrap();

        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn messages_without_id_always_processed() {
        let counter = Arc::new(AtomicU32::new(0));
        let mw = DedupMiddleware {
            config: DedupConfig::default(),
            seen: Arc::new(parking_lot::Mutex::new(HashMap::new())),
        };
        let handler = chain_handlers(
            counting_handler(&counter),
            &[Arc::new(mw) as Arc<dyn HandlerMiddleware<String>>],
        );

        let msg1 = Message::new("t", "a".to_string());
        let msg2 = Message::new("t", "b".to_string());

        handler.handle(msg1).await.unwrap();
        handler.handle(msg2).await.unwrap();

        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn window_size_evicts_oldest() {
        let counter = Arc::new(AtomicU32::new(0));
        let mw = DedupMiddleware {
            config: DedupConfig {
                window_size: 2,
                ttl: Duration::from_secs(300),
            },
            seen: Arc::new(parking_lot::Mutex::new(HashMap::new())),
        };
        let handler = chain_handlers(
            counting_handler(&counter),
            &[Arc::new(mw) as Arc<dyn HandlerMiddleware<String>>],
        );

        // Fill window with id-1 and id-2.
        handler
            .handle(Message::new("t", "a".to_string()).with_header("message-id", "id-1"))
            .await
            .unwrap();
        handler
            .handle(Message::new("t", "b".to_string()).with_header("message-id", "id-2"))
            .await
            .unwrap();

        // id-3 should evict id-1 (the oldest).
        handler
            .handle(Message::new("t", "c".to_string()).with_header("message-id", "id-3"))
            .await
            .unwrap();

        // id-1 should now be accepted again because it was evicted.
        handler
            .handle(Message::new("t", "d".to_string()).with_header("message-id", "id-1"))
            .await
            .unwrap();

        assert_eq!(counter.load(Ordering::SeqCst), 4);
    }
}
