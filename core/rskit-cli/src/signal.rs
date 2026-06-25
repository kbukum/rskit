//! Ctrl+C / graceful shutdown over [`tokio_util::sync::CancellationToken`].
//!
//! rskit standardizes on `tokio_util`'s [`CancellationToken`] as its single
//! cooperative-cancellation type: `rskit-worker` handlers and `rskit-process`
//! already consume it directly, so the CLI layer uses the *same* type end-to-end
//! rather than introducing a parallel wrapper. The token is re-exported here so a
//! CLI can name it without depending on `tokio-util` directly, and [`on_ctrl_c`]
//! installs the one-shot interrupt handler that drives it.

pub use tokio_util::sync::CancellationToken;

/// Install a first-Ctrl+C handler and return the cooperative cancellation token.
///
/// Must be called from within a Tokio runtime: a background task awaits the
/// interrupt signal and cancels the returned token on the first Ctrl+C. Clone the
/// token and hand it to spawned tasks, an `rskit-worker` handler, or an
/// `rskit-process` call; holders observe `is_cancelled()` / `cancelled()` and
/// wind down gracefully.
#[must_use]
pub fn on_ctrl_c() -> CancellationToken {
    let token = CancellationToken::new();
    let child = token.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            child.cancel();
        }
    });
    token
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancellation_propagates_to_clones() {
        let token = CancellationToken::new();
        let clone = token.clone();

        assert!(!token.is_cancelled());
        assert!(!clone.is_cancelled());

        token.cancel();

        assert!(token.is_cancelled());
        assert!(clone.is_cancelled());
    }

    #[tokio::test]
    async fn on_ctrl_c_returns_a_live_uncancelled_token() {
        let token = on_ctrl_c();
        assert!(!token.is_cancelled());
        // Cancelling locally still works; the interrupt task is just one source.
        token.cancel();
        token.cancelled().await;
        assert!(token.is_cancelled());
    }
}
