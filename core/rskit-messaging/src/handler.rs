//! Message handler trait and middleware chain for consumed messages.

use std::sync::Arc;

use async_trait::async_trait;
use rskit_errors::AppResult;

use crate::message::Message;

/// Handler for processing consumed messages.
///
/// Implement this trait to define how incoming messages are processed. For simple cases,
/// use [`FnHandler`] to wrap a closure.
#[async_trait]
pub trait MessageHandler<T: Send + Sync + 'static>: Send + Sync + 'static {
    /// Process a single message.
    async fn handle(&self, msg: Message<T>) -> AppResult<()>;
}

/// Middleware that wraps a handler with cross-cutting concerns.
///
/// Each middleware receives the next handler in the chain
/// and returns a new handler that adds behaviour around it (logging, metrics, error recovery, etc.).
pub trait HandlerMiddleware<T: Send + Sync + 'static>: Send + Sync + 'static {
    /// Wrap the given handler, returning a new handler that adds middleware behaviour.
    fn wrap(&self, next: Arc<dyn MessageHandler<T>>) -> Arc<dyn MessageHandler<T>>;
}

/// Chains middleware around a base handler.
///
/// Middlewares are applied in order: the first middleware in the slice becomes the outermost wrapper.
///
/// ```text
/// chain_handlers(base, [mw_a, mw_b])
///   => mw_a(mw_b(base))
/// ```
pub fn chain_handlers<T: Send + Sync + 'static>(
    base: Arc<dyn MessageHandler<T>>,
    middlewares: &[Arc<dyn HandlerMiddleware<T>>],
) -> Arc<dyn MessageHandler<T>> {
    let mut handler = base;
    for mw in middlewares.iter().rev() {
        handler = mw.wrap(handler);
    }
    handler
}

/// Adapter that turns a closure into a [`HandlerMiddleware`].
///
/// Useful for simple one-off middleware that do not need their own struct.
///
/// # Example
///
/// ```rust,ignore
/// use rskit_messaging::handler::middleware_fn;
/// use std::sync::Arc;
///
/// let mw = middleware_fn(|next| {
///     // wrap next handler with custom behaviour
///     next
/// });
/// ```
pub fn middleware_fn<T, F>(f: F) -> impl HandlerMiddleware<T>
where
    T: Send + Sync + 'static,
    F: Fn(Arc<dyn MessageHandler<T>>) -> Arc<dyn MessageHandler<T>> + Send + Sync + 'static,
{
    FnMiddleware {
        func: f,
        _marker: std::marker::PhantomData,
    }
}

struct FnMiddleware<T, F> {
    func: F,
    _marker: std::marker::PhantomData<T>,
}

impl<T, F> HandlerMiddleware<T> for FnMiddleware<T, F>
where
    T: Send + Sync + 'static,
    F: Fn(Arc<dyn MessageHandler<T>>) -> Arc<dyn MessageHandler<T>> + Send + Sync + 'static,
{
    fn wrap(&self, next: Arc<dyn MessageHandler<T>>) -> Arc<dyn MessageHandler<T>> {
        (self.func)(next)
    }
}

/// Adapter that turns an async closure into a [`MessageHandler`].
///
/// # Example
///
/// ```rust,ignore
/// use rskit_messaging::handler::FnHandler;
///
/// let handler = FnHandler::new(|msg| async move {
///     println!("got: {:?}", msg.payload);
///     Ok(())
/// });
/// ```
pub struct FnHandler<T, F, Fut>
where
    T: Send + Sync + 'static,
    F: Fn(Message<T>) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = AppResult<()>> + Send + 'static,
{
    func: F,
    _marker: std::marker::PhantomData<T>,
}

impl<T, F, Fut> FnHandler<T, F, Fut>
where
    T: Send + Sync + 'static,
    F: Fn(Message<T>) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = AppResult<()>> + Send + 'static,
{
    /// Create a new function handler from the given closure.
    pub const fn new(func: F) -> Self {
        Self {
            func,
            _marker: std::marker::PhantomData,
        }
    }
}

#[async_trait]
impl<T, F, Fut> MessageHandler<T> for FnHandler<T, F, Fut>
where
    T: Send + Sync + 'static,
    F: Fn(Message<T>) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = AppResult<()>> + Send + 'static,
{
    async fn handle(&self, msg: Message<T>) -> AppResult<()> {
        (self.func)(msg).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    #[tokio::test]
    async fn fn_handler_processes_message() {
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();
        let handler = FnHandler::new(move |_msg: Message<String>| {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        });

        let msg = Message::new("t", "hello".to_string());
        handler.handle(msg).await.unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    /// A middleware that increments a counter before delegating.
    struct CountingMiddleware {
        counter: Arc<AtomicU32>,
    }

    struct CountingHandler<T: Send + Sync + 'static> {
        counter: Arc<AtomicU32>,
        next: Arc<dyn MessageHandler<T>>,
    }

    #[async_trait]
    impl<T: Send + Sync + 'static> MessageHandler<T> for CountingHandler<T> {
        async fn handle(&self, msg: Message<T>) -> AppResult<()> {
            self.counter.fetch_add(1, Ordering::SeqCst);
            self.next.handle(msg).await
        }
    }

    impl<T: Send + Sync + 'static> HandlerMiddleware<T> for CountingMiddleware {
        fn wrap(&self, next: Arc<dyn MessageHandler<T>>) -> Arc<dyn MessageHandler<T>> {
            Arc::new(CountingHandler {
                counter: self.counter.clone(),
                next,
            })
        }
    }

    #[tokio::test]
    async fn middleware_chain_applies_in_order() {
        let mw_counter = Arc::new(AtomicU32::new(0));
        let base_counter = Arc::new(AtomicU32::new(0));

        let bc = base_counter.clone();
        let base: Arc<dyn MessageHandler<String>> =
            Arc::new(FnHandler::new(move |_msg: Message<String>| {
                let bc = bc.clone();
                async move {
                    bc.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            }));

        let mw: Arc<dyn HandlerMiddleware<String>> = Arc::new(CountingMiddleware {
            counter: mw_counter.clone(),
        });

        let chained = chain_handlers(base, &[mw]);
        let msg = Message::new("t", "data".to_string());
        chained.handle(msg).await.unwrap();

        assert_eq!(mw_counter.load(Ordering::SeqCst), 1);
        assert_eq!(base_counter.load(Ordering::SeqCst), 1);
    }
}
