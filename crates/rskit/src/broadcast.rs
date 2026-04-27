//! `LossyBroadcast` — a broadcast receiver wrapper that increments a metric counter
//! when messages are dropped due to lag, rather than silently losing them.

use tokio::sync::broadcast;

/// A broadcast receiver that tracks and logs dropped messages.
///
/// Wraps [`tokio::sync::broadcast::Receiver`] and emits a `tracing::warn!` event
/// (including the drop count) whenever the receiver falls behind and messages are
/// discarded due to channel lag. This makes lag observable in structured logs and
/// distributed traces instead of silently losing data.
///
/// # Examples
///
/// ```rust,ignore
/// let (tx, rx) = tokio::sync::broadcast::channel(16);
/// let mut receiver = LossyBroadcast::new(rx);
///
/// while let Some(msg) = receiver.recv().await {
///     println!("received: {:?}", msg);
/// }
/// ```
pub struct LossyBroadcast<T> {
    rx: broadcast::Receiver<T>,
}

impl<T: Clone> LossyBroadcast<T> {
    /// Wrap an existing broadcast receiver.
    pub fn new(rx: broadcast::Receiver<T>) -> Self {
        Self { rx }
    }

    /// Receive the next message, logging any lagged (dropped) messages.
    ///
    /// Returns `None` when the channel is closed.
    pub async fn recv(&mut self) -> Option<T> {
        loop {
            match self.rx.recv().await {
                Ok(value) => return Some(value),
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(dropped = n, "broadcast receiver lagged — {n} messages dropped");
                }
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }
}
