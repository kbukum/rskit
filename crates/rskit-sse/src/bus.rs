use std::convert::Infallible;

use axum::response::sse::Event;
use futures_util::stream::{Stream, StreamExt};
use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::Serialize;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

/// A broadcast-based Server-Sent Events bus.
///
/// `SseBus<T>` wraps a `tokio::sync::broadcast` channel and provides
/// helpers to publish events and create SSE-compatible subscriber streams.
pub struct SseBus<T: Clone + Send + Sync + 'static> {
    tx: broadcast::Sender<T>,
}

impl<T: Clone + Send + Sync + Serialize + 'static> SseBus<T> {
    /// Create a new SSE bus with the given channel capacity.
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    /// Publish an event to all active subscribers.
    pub fn publish(&self, event: T) -> AppResult<()> {
        self.tx.send(event).map_err(|_| {
            AppError::new(
                ErrorCode::Internal,
                "SSE publish failed: no active subscribers",
            )
        })?;
        Ok(())
    }

    /// Create a new subscriber stream suitable for use with axum's `Sse` response.
    ///
    /// Each item in the stream is a `Result<Event, Infallible>`, where the event
    /// data is the JSON-serialized form of `T`.
    pub fn subscribe(&self) -> impl Stream<Item = Result<Event, Infallible>> {
        let rx = self.tx.subscribe();
        BroadcastStream::new(rx).filter_map(|result| async move {
            match result {
                Ok(item) => match serde_json::to_string(&item) {
                    Ok(json) => Some(Ok(Event::default().data(json))),
                    Err(err) => {
                        tracing::warn!(error = %err, "SSE serialization failed, skipping event");
                        None
                    }
                },
                Err(err) => {
                    tracing::warn!(error = %err, "SSE subscriber receive error, skipping");
                    None
                }
            }
        })
    }

    /// Return the number of active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;

    #[derive(Clone, Debug, Serialize)]
    struct TestEvent {
        msg: String,
    }

    #[tokio::test]
    async fn publish_and_subscribe() {
        let bus = SseBus::new(16);
        let mut stream = std::pin::pin!(bus.subscribe());

        bus.publish(TestEvent {
            msg: "hello".into(),
        })
        .unwrap();

        let event = tokio::time::timeout(std::time::Duration::from_secs(1), stream.next())
            .await
            .expect("timeout")
            .expect("stream ended");

        assert!(event.is_ok());
    }

    #[test]
    fn subscriber_count_tracks_receivers() {
        let bus: SseBus<TestEvent> = SseBus::new(16);
        assert_eq!(bus.subscriber_count(), 0);

        let _s1 = bus.subscribe();
        // BroadcastStream subscribes on creation
        assert_eq!(bus.subscriber_count(), 1);
    }
}
