//! Tracing span middleware for message handlers.

use std::sync::Arc;

use async_trait::async_trait;
use rskit_errors::AppResult;

use crate::handler::{HandlerMiddleware, MessageHandler};
use crate::message::Message;

/// Create a middleware that wraps each handler invocation in a
/// [`tracing`] span containing the message topic and key.
pub fn tracing_middleware<T: Send + Sync + 'static>() -> impl HandlerMiddleware<T> {
    TracingMiddleware {
        _marker: std::marker::PhantomData,
    }
}

struct TracingMiddleware<T> {
    _marker: std::marker::PhantomData<T>,
}

impl<T: Send + Sync + 'static> HandlerMiddleware<T> for TracingMiddleware<T> {
    fn wrap(&self, next: Arc<dyn MessageHandler<T>>) -> Arc<dyn MessageHandler<T>> {
        Arc::new(TracingHandler { next })
    }
}

struct TracingHandler<T: Send + Sync + 'static> {
    next: Arc<dyn MessageHandler<T>>,
}

#[async_trait]
impl<T: Send + Sync + 'static> MessageHandler<T> for TracingHandler<T> {
    async fn handle(&self, msg: Message<T>) -> AppResult<()> {
        let topic = msg.topic.clone();
        let key = msg.key.as_deref().unwrap_or("").to_string();
        let span = ::tracing::info_span!(
            "message.handle",
            messaging.topic = %topic,
            messaging.key = %key,
        );
        let _enter = span.enter();
        self.next.handle(msg).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handler::{FnHandler, chain_handlers};

    #[tokio::test]
    async fn tracing_middleware_invokes_next_handler() {
        use std::sync::atomic::{AtomicU32, Ordering};

        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();
        let base: Arc<dyn MessageHandler<String>> =
            Arc::new(FnHandler::new(move |_msg: Message<String>| {
                let c = c.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            }));

        let mw: Arc<dyn HandlerMiddleware<String>> = Arc::new(tracing_middleware());
        let handler = chain_handlers(base, &[mw]);

        handler
            .handle(Message::new("topic", "data".to_string()))
            .await
            .unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}
