//! Bounded fan-out broadcaster source.
//!
//! [`Broadcaster<T>`] turns "observe a backend" into a *bounded, cancellable
//! stream of typed events* fanned out to many independent subscribers. Each
//! subscriber owns a private bounded channel: a subscriber that falls further
//! behind than the buffer loses interim events (backpressure by drop) but never
//! blocks the broadcaster or its peers. Disconnected subscribers are pruned
//! lazily on the next broadcast.
//!
//! This is the canonical owner for the "watch a source → typed change stream"
//! shape that recurs across config reloads, service discovery, cache
//! invalidation, and secret rotation. Consumers that must not miss events should
//! treat any received item as a signal to re-run their full load pipeline.

use std::fmt;
use std::pin::Pin;
use std::sync::Arc;

use futures::Stream;
use parking_lot::Mutex;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;

/// A bounded, owned, cancellable stream of broadcast events.
///
/// Yielded by [`Broadcaster::subscribe`]; terminates when the originating
/// [`CancellationToken`] fires or the broadcaster is dropped.
pub type BroadcastStream<T> = Pin<Box<dyn Stream<Item = T> + Send + 'static>>;

/// Default per-subscriber buffer used by [`Broadcaster::new`].
///
/// A subscriber lagging more than this many unconsumed events drops the
/// overflow rather than stalling the broadcaster.
pub const DEFAULT_BROADCAST_BUFFER: usize = 64;

/// A bounded, cancellable fan-out broadcaster.
///
/// Cheaply cloneable; every clone shares the same subscriber set, so an event
/// broadcast through any clone reaches all live subscribers. `T` must be
/// `Clone` (each subscriber receives its own copy) and `Send + 'static` to
/// cross the per-subscriber channel.
pub struct Broadcaster<T> {
    senders: Arc<Mutex<Vec<mpsc::Sender<T>>>>,
    buffer: usize,
}

impl<T> Clone for Broadcaster<T> {
    fn clone(&self) -> Self {
        Self {
            senders: Arc::clone(&self.senders),
            buffer: self.buffer,
        }
    }
}

impl<T> fmt::Debug for Broadcaster<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Broadcaster")
            .field("buffer", &self.buffer)
            .field("subscribers", &self.subscriber_count())
            .finish()
    }
}

impl<T> Default for Broadcaster<T>
where
    T: Clone + Send + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Broadcaster<T>
where
    T: Clone + Send + 'static,
{
    /// Create a broadcaster with the [`DEFAULT_BROADCAST_BUFFER`] per-subscriber
    /// buffer.
    #[must_use]
    pub fn new() -> Self {
        Self::with_buffer(DEFAULT_BROADCAST_BUFFER)
    }

    /// Create a broadcaster with an explicit per-subscriber buffer.
    ///
    /// A `buffer` of zero is clamped to one so every subscriber can hold at
    /// least one in-flight event.
    #[must_use]
    pub fn with_buffer(buffer: usize) -> Self {
        Self {
            senders: Arc::new(Mutex::new(Vec::new())),
            buffer: buffer.max(1),
        }
    }
}

impl<T> Broadcaster<T> {
    /// The per-subscriber buffer capacity.
    #[must_use]
    pub fn buffer(&self) -> usize {
        self.buffer
    }

    /// The number of currently registered subscribers.
    ///
    /// Disconnected subscribers are pruned lazily on [`broadcast`](Self::broadcast),
    /// so this count may transiently include subscribers whose streams have been
    /// dropped but not yet pruned.
    #[must_use]
    pub fn subscriber_count(&self) -> usize {
        self.senders.lock().len()
    }

    /// Register a new subscriber and return its bounded, cancellable stream.
    ///
    /// The returned stream yields events broadcast after subscription and
    /// terminates once `cancel` fires or the broadcaster is dropped. Pass a
    /// fresh [`CancellationToken`] when no external cancellation is required.
    pub fn subscribe(&self, cancel: CancellationToken) -> BroadcastStream<T>
    where
        T: Send + 'static,
    {
        use futures::StreamExt as _;

        let (tx, rx) = mpsc::channel(self.buffer);
        self.senders.lock().push(tx);
        Box::pin(ReceiverStream::new(rx).take_until(cancel.cancelled_owned()))
    }

    /// Fan `item` out to every live subscriber.
    ///
    /// Each subscriber receives an independent clone. A subscriber whose buffer
    /// is full is retained but skips this event (bounded drop backpressure); a
    /// subscriber whose stream has been dropped is pruned.
    pub fn broadcast(&self, item: &T)
    where
        T: Clone,
    {
        self.senders
            .lock()
            .retain(|tx| match tx.try_send(item.clone()) {
                Ok(()) => true,
                Err(mpsc::error::TrySendError::Full(_)) => true,
                Err(mpsc::error::TrySendError::Closed(_)) => false,
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt as _;

    #[tokio::test]
    async fn delivers_to_all_subscribers() {
        let bc = Broadcaster::<u32>::new();
        let cancel = CancellationToken::new();
        let mut a = bc.subscribe(cancel.clone());
        let mut b = bc.subscribe(cancel.clone());

        bc.broadcast(&7);

        assert_eq!(a.next().await, Some(7));
        assert_eq!(b.next().await, Some(7));
    }

    #[tokio::test]
    async fn stream_terminates_on_cancel() {
        let bc = Broadcaster::<u32>::new();
        let cancel = CancellationToken::new();
        let mut sub = bc.subscribe(cancel.clone());

        bc.broadcast(&1);
        assert_eq!(sub.next().await, Some(1));

        cancel.cancel();
        assert_eq!(sub.next().await, None);
    }

    #[tokio::test]
    async fn dropped_subscriber_is_pruned() {
        let bc = Broadcaster::<u32>::new();
        let cancel = CancellationToken::new();
        let sub = bc.subscribe(cancel.clone());
        assert_eq!(bc.subscriber_count(), 1);

        drop(sub);
        // Prune happens lazily on the next broadcast.
        bc.broadcast(&1);
        assert_eq!(bc.subscriber_count(), 0);
    }

    #[tokio::test]
    async fn full_subscriber_drops_overflow_without_blocking() {
        let bc = Broadcaster::<u32>::with_buffer(2);
        let cancel = CancellationToken::new();
        let mut sub = bc.subscribe(cancel.clone());

        // Send more than the buffer; overflow is dropped, not blocked.
        for i in 0..5 {
            bc.broadcast(&i);
        }

        let mut received = Vec::new();
        // Only the first `buffer` events are retained.
        while let Ok(Some(v)) =
            tokio::time::timeout(std::time::Duration::from_millis(50), sub.next()).await
        {
            received.push(v);
        }
        assert_eq!(received, vec![0, 1]);
    }

    #[tokio::test]
    async fn zero_buffer_is_clamped_to_one() {
        let bc = Broadcaster::<u32>::with_buffer(0);
        assert_eq!(bc.buffer(), 1);
    }

    #[tokio::test]
    async fn clones_share_subscriber_set() {
        let bc = Broadcaster::<u32>::new();
        let cancel = CancellationToken::new();
        let mut sub = bc.subscribe(cancel.clone());

        let clone = bc.clone();
        clone.broadcast(&42);

        assert_eq!(sub.next().await, Some(42));
    }
}
