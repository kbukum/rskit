//! Ctrl+C / graceful shutdown via CancellationToken.
//!
//! Wraps `tokio_util::sync::CancellationToken` with a simpler API.

/// Token for cooperative cancellation across async tasks.
///
/// Clone and pass to spawned tasks. When `cancel()` is called (or Ctrl+C fires),
/// all holders see `is_cancelled() == true` and can wind down gracefully.
#[derive(Clone)]
pub struct CancellationToken {
    inner: tokio_util::sync::CancellationToken,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            inner: tokio_util::sync::CancellationToken::new(),
        }
    }

    /// Signal cancellation to all holders of this token.
    pub fn cancel(&self) {
        self.inner.cancel();
    }

    /// Check whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.inner.is_cancelled()
    }

    /// Wait until cancellation is requested.
    pub async fn cancelled(&self) {
        self.inner.cancelled().await;
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancellation_propagation() {
        let token = CancellationToken::new();
        let clone = token.clone();

        assert!(!token.is_cancelled());
        assert!(!clone.is_cancelled());

        token.cancel();

        assert!(token.is_cancelled());
        assert!(clone.is_cancelled());
    }
}
