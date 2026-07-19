//! Topic-based message routing with wildcard pattern support.

use std::sync::Arc;

use async_trait::async_trait;
use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::handler::MessageHandler;
use crate::message::Message;

/// Routes incoming messages to handlers based on topic patterns.
///
/// Supports exact-match and simple wildcard patterns where `*` matches any sequence of characters.
///
/// # Example
///
/// ```rust,ignore
/// use rskit_messaging::router::MessageRouter;
///
/// let router = MessageRouter::<String>::new()
///     .handle("orders.created", order_handler)
///     .handle("orders.*", fallback_handler)
///     .default_handler(catch_all)
///     .build();
/// ```
pub struct MessageRouter<T: Send + Sync + 'static> {
    routes: Vec<(String, Arc<dyn MessageHandler<T>>)>,
    default: Option<Arc<dyn MessageHandler<T>>>,
}

impl<T: Send + Sync + 'static> MessageRouter<T> {
    /// Create a new empty router.
    pub fn new() -> Self {
        Self {
            routes: Vec::new(),
            default: None,
        }
    }

    /// Register a handler for the given topic pattern.
    ///
    /// Patterns support `*` as a wildcard that matches any sequence of characters. For example,
    /// `"content.*"` matches `"content.discovered"`. Patterns are evaluated in registration order;
    /// the first match wins.
    #[must_use]
    pub fn handle(mut self, pattern: &str, handler: Arc<dyn MessageHandler<T>>) -> Self {
        self.routes.push((pattern.to_string(), handler));
        self
    }

    /// Register a default handler for messages that match no pattern.
    #[must_use]
    pub fn default_handler(mut self, handler: Arc<dyn MessageHandler<T>>) -> Self {
        self.default = Some(handler);
        self
    }

    /// Build an [`Arc<dyn MessageHandler<T>>`] that dispatches messages according to the registered patterns.
    pub fn build(self) -> Arc<dyn MessageHandler<T>> {
        Arc::new(RouterHandler {
            routes: self.routes,
            default: self.default,
        })
    }
}

impl<T: Send + Sync + 'static> Default for MessageRouter<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Internal handler produced by [`MessageRouter::build`].
struct RouterHandler<T: Send + Sync + 'static> {
    routes: Vec<(String, Arc<dyn MessageHandler<T>>)>,
    default: Option<Arc<dyn MessageHandler<T>>>,
}

#[async_trait]
impl<T: Send + Sync + 'static> MessageHandler<T> for RouterHandler<T> {
    async fn handle(&self, msg: Message<T>) -> AppResult<()> {
        for (pattern, handler) in &self.routes {
            if matches_pattern(pattern, &msg.topic) {
                return handler.handle(msg).await;
            }
        }
        if let Some(default) = &self.default {
            return default.handle(msg).await;
        }
        Err(AppError::new(
            ErrorCode::NotFound,
            format!("no handler for topic '{}'", msg.topic),
        ))
    }
}

/// Check whether `topic` matches the given `pattern`.
///
/// `*` in the pattern matches any sequence of characters (including none).
fn matches_pattern(pattern: &str, topic: &str) -> bool {
    if !pattern.contains('*') {
        return pattern == topic;
    }
    wildcard_match(pattern.as_bytes(), topic.as_bytes())
}

/// Recursive wildcard matching for patterns with `*`.
fn wildcard_match(pattern: &[u8], text: &[u8]) -> bool {
    match (pattern.first(), text.first()) {
        (None, None) => true,
        (Some(b'*'), _) => {
            wildcard_match(&pattern[1..], text)
                || (!text.is_empty() && wildcard_match(pattern, &text[1..]))
        }
        (Some(p), Some(t)) if p == t => wildcard_match(&pattern[1..], &text[1..]),
        (Some(_), None) => pattern.iter().all(|&b| b == b'*'),
        (_, Some(_)) => false,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;
    use crate::handler::FnHandler;

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

    #[test]
    fn pattern_exact_match() {
        assert!(matches_pattern("orders.created", "orders.created"));
        assert!(!matches_pattern("orders.created", "orders.updated"));
    }

    #[test]
    fn pattern_wildcard_suffix() {
        assert!(matches_pattern("content.*", "content.discovered"));
        assert!(matches_pattern("content.*", "content.updated"));
        assert!(matches_pattern("content.*", "content."));
        assert!(!matches_pattern("content.*", "other.discovered"));
    }

    #[test]
    fn pattern_wildcard_prefix() {
        assert!(matches_pattern("*.events", "user.events"));
        assert!(matches_pattern("*.events", "system.events"));
        assert!(!matches_pattern("*.events", "user.logs"));
    }

    #[test]
    fn pattern_wildcard_only() {
        assert!(matches_pattern("*", "anything"));
        assert!(matches_pattern("*", ""));
    }

    #[tokio::test]
    async fn router_exact_match() {
        let counter = Arc::new(AtomicU32::new(0));
        let router = MessageRouter::<String>::new()
            .handle("orders.created", counting_handler(&counter))
            .build();

        let msg = Message::new("orders.created", "data".to_string());
        router.handle(msg).await.unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn router_wildcard_match() {
        let counter = Arc::new(AtomicU32::new(0));
        let router = MessageRouter::<String>::new()
            .handle("content.*", counting_handler(&counter))
            .build();

        let msg = Message::new("content.discovered", "data".to_string());
        router.handle(msg).await.unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn router_default_handler() {
        let specific = Arc::new(AtomicU32::new(0));
        let default = Arc::new(AtomicU32::new(0));
        let router = MessageRouter::<String>::new()
            .handle("orders.*", counting_handler(&specific))
            .default_handler(counting_handler(&default))
            .build();

        let msg = Message::new("unknown.topic", "data".to_string());
        router.handle(msg).await.unwrap();
        assert_eq!(specific.load(Ordering::SeqCst), 0);
        assert_eq!(default.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn router_no_match_returns_error() {
        let router = MessageRouter::<String>::new().build();

        let msg = Message::new("unknown", "data".to_string());
        let result = router.handle(msg).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn router_first_match_wins() {
        let first = Arc::new(AtomicU32::new(0));
        let second = Arc::new(AtomicU32::new(0));

        let router = MessageRouter::<String>::new()
            .handle("orders.*", counting_handler(&first))
            .handle("orders.created", counting_handler(&second))
            .build();

        let msg = Message::new("orders.created", "data".to_string());
        router.handle(msg).await.unwrap();
        assert_eq!(first.load(Ordering::SeqCst), 1);
        assert_eq!(second.load(Ordering::SeqCst), 0);
    }
}
