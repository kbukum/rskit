//! Middleware stack builder for composing consumer handler pipelines.
//!
//! [`StackBuilder`] provides a fluent API to configure middleware and build
//! a fully-wrapped handler in a standard order.
//!
//! # Example
//!
//! ```rust,ignore
//! use rskit_messaging::middleware::StackBuilder;
//!
//! let handler = StackBuilder::<Vec<u8>>::new(base_handler)
//!     .with_retry(retry_config)
//!     .with_metrics(metrics, "my-topic")
//!     .build();
//! ```

use std::sync::Arc;

use crate::handler::{HandlerMiddleware, MessageHandler, chain_handlers};

/// Fluent builder for composing messaging middleware into a handler pipeline.
///
/// Middleware is applied in the order added — the first middleware added
/// becomes the outermost wrapper and runs first on each message.
///
/// # Example
///
/// ```rust,ignore
/// let stack = StackBuilder::new(base_handler)
///     .with(retry_mw)
///     .with(metrics_mw)
///     .with(dedup_mw)
///     .build();
/// ```
pub struct StackBuilder<T: Send + Sync + Clone + 'static> {
    base: Arc<dyn MessageHandler<T>>,
    middlewares: Vec<Arc<dyn HandlerMiddleware<T>>>,
}

impl<T: Send + Sync + Clone + 'static> StackBuilder<T> {
    /// Create a new builder wrapping the given base handler.
    pub fn new(base: Arc<dyn MessageHandler<T>>) -> Self {
        Self {
            base,
            middlewares: Vec::new(),
        }
    }

    /// Add middleware to the stack.
    ///
    /// Middleware is applied in the order added — the first middleware
    /// added becomes the outermost wrapper.
    #[must_use]
    pub fn with<M: HandlerMiddleware<T> + 'static>(mut self, mw: M) -> Self {
        self.middlewares.push(Arc::new(mw));
        self
    }

    /// Build the fully-wrapped handler by applying all middleware.
    pub fn build(self) -> Arc<dyn MessageHandler<T>> {
        chain_handlers(self.base, &self.middlewares)
    }
}
