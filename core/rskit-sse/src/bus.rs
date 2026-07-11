use std::collections::VecDeque;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::response::sse::Event;
use futures_util::stream::{self, Stream, StreamExt};
use parking_lot::Mutex;
use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::Serialize;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

const MAX_CAPACITY: usize = 1 << 24;

/// A toolkit-native SSE event with replay metadata.
#[derive(Debug, Clone)]
pub struct SseEvent<T> {
    /// Monotonic event id assigned by the bus.
    pub id: String,
    /// Optional SSE event type.
    pub event: Option<String>,
    /// Optional client retry interval.
    pub retry: Option<Duration>,
    /// Event payload.
    pub data: T,
}

impl<T> SseEvent<T> {
    /// Convert this event into an axum SSE event.
    ///
    /// # Errors
    /// Returns an error when the payload cannot be serialized as JSON.
    pub fn into_axum_event(self) -> AppResult<Event>
    where
        T: Serialize,
    {
        let data = serde_json::to_string(&self.data).map_err(|error| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("failed to serialize SSE event: {error}"),
            )
        })?;
        let mut event = Event::default().id(self.id).data(data);
        if let Some(kind) = self.event {
            event = event.event(kind);
        }
        if let Some(retry) = self.retry {
            event = event.retry(retry);
        }
        Ok(event)
    }
}

/// A bounded Server-Sent Events bus.
///
/// Live subscriber fan-out is bounded by the configured broadcast capacity. A
/// bounded replay buffer of the same size stores recent events for
/// `Last-Event-ID` resume. Slow live subscribers skip lagged events and receive
/// the newest available events.
pub struct SseBus<T: Clone + Send + Sync + 'static> {
    tx: broadcast::Sender<SseEvent<T>>,
    state: Arc<Mutex<SseState<T>>>,
    capacity: usize,
    retry: Option<Duration>,
}

struct SseState<T> {
    replay: VecDeque<SseEvent<T>>,
    next_id: u64,
}

impl<T: Clone + Send + Sync + Serialize + 'static> SseBus<T> {
    /// Create a new SSE bus with the given bounded channel capacity.
    ///
    /// # Errors
    /// Returns an error when `capacity` is zero or larger than the toolkit maximum.
    pub fn new(capacity: usize) -> AppResult<Self> {
        validate_capacity(capacity)?;
        let (tx, _) = broadcast::channel(capacity);
        Ok(Self {
            tx,
            state: Arc::new(Mutex::new(SseState {
                replay: VecDeque::with_capacity(capacity),
                next_id: 1,
            })),
            capacity,
            retry: None,
        })
    }

    /// Configure the retry interval attached to subsequently published events.
    #[must_use]
    pub const fn with_retry(mut self, retry: Duration) -> Self {
        self.retry = Some(retry);
        self
    }

    /// Publish an event to the replay buffer and all active subscribers.
    ///
    /// Publishing without subscribers is successful; the event remains available
    /// for bounded replay until it is evicted by newer events.
    pub fn publish(&self, data: T) -> AppResult<SseEvent<T>> {
        let mut state = self.state.lock();
        let event = SseEvent {
            id: state.next_id.to_string(),
            event: None,
            retry: self.retry,
            data,
        };
        state.next_id += 1;
        push_replay(&mut state.replay, self.capacity, event.clone());
        drop(state);
        let _ = self.tx.send(event.clone());
        Ok(event)
    }

    /// Create a live subscriber stream of toolkit-native SSE events.
    pub fn subscribe(&self) -> impl Stream<Item = Result<SseEvent<T>, Infallible>> {
        self.live_stream()
    }

    /// Create a subscriber stream that first replays events after `last_event_id`.
    pub fn subscribe_after(
        &self,
        last_event_id: Option<&str>,
    ) -> impl Stream<Item = Result<SseEvent<T>, Infallible>> {
        let (replay, rx) = {
            let state = self.state.lock();
            // Hold the publish state lock across snapshot + subscribe so no event can be
            // published between replay collection and live receiver creation.
            let replay = replay_after(&state.replay, last_event_id);
            let rx = self.tx.subscribe();
            drop(state);
            (replay, rx)
        };
        stream::iter(replay.into_iter().map(Ok)).chain(live_stream_from(rx))
    }

    /// Create an axum-compatible SSE stream adapter.
    pub fn subscribe_axum(&self) -> impl Stream<Item = Result<Event, Infallible>> {
        self.subscribe()
            .filter_map(|result| async move { axum_event(result) })
    }

    /// Create an axum-compatible SSE stream adapter with bounded replay.
    pub fn subscribe_axum_after(
        &self,
        last_event_id: Option<&str>,
    ) -> impl Stream<Item = Result<Event, Infallible>> {
        self.subscribe_after(last_event_id)
            .filter_map(|result| async move { axum_event(result) })
    }

    /// Return the number of active subscribers.
    #[must_use]
    pub fn subscriber_count(&self) -> usize {
        self.tx.receiver_count()
    }

    fn live_stream(&self) -> impl Stream<Item = Result<SseEvent<T>, Infallible>> {
        live_stream_from(self.tx.subscribe())
    }
}

fn push_replay<T>(replay: &mut VecDeque<SseEvent<T>>, capacity: usize, event: SseEvent<T>) {
    if replay.len() == capacity {
        replay.pop_front();
    }
    replay.push_back(event);
}

fn replay_after<T: Clone>(
    replay: &VecDeque<SseEvent<T>>,
    last_event_id: Option<&str>,
) -> Vec<SseEvent<T>> {
    let last = last_event_id.and_then(|value| value.parse::<u64>().ok());
    replay
        .iter()
        .filter(|event| last.is_none_or(|last| event.id.parse::<u64>().is_ok_and(|id| id > last)))
        .cloned()
        .collect()
}

fn live_stream_from<T: Clone + Send + Sync + 'static>(
    rx: broadcast::Receiver<SseEvent<T>>,
) -> impl Stream<Item = Result<SseEvent<T>, Infallible>> {
    BroadcastStream::new(rx).filter_map(|result| async move {
        match result {
            Ok(item) => Some(Ok(item)),
            Err(err) => {
                tracing::warn!(error = %err, "SSE subscriber lagged; skipping missed events");
                None
            }
        }
    })
}

fn validate_capacity(capacity: usize) -> AppResult<()> {
    if capacity == 0 {
        return Err(AppError::invalid_input(
            "capacity",
            "SSE bus capacity must be greater than zero",
        ));
    }
    if capacity > MAX_CAPACITY {
        return Err(AppError::invalid_input(
            "capacity",
            format!("SSE bus capacity must be at most {MAX_CAPACITY}"),
        ));
    }
    Ok(())
}

fn axum_event<T: Serialize>(
    result: Result<SseEvent<T>, Infallible>,
) -> Option<Result<Event, Infallible>> {
    match result {
        Ok(event) => match event.into_axum_event() {
            Ok(event) => Some(Ok(event)),
            Err(error) => {
                tracing::warn!(error = %error, "SSE serialization failed; skipping event");
                None
            }
        },
        Err(infallible) => match infallible {},
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

    #[derive(Clone, Debug)]
    struct FailingEvent;

    impl Serialize for FailingEvent {
        fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            Err(serde::ser::Error::custom("synthetic serialization failure"))
        }
    }

    #[tokio::test]
    async fn publish_and_subscribe() {
        let bus = SseBus::new(16).unwrap();
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
        let bus: SseBus<TestEvent> = SseBus::new(16).unwrap();
        assert_eq!(bus.subscriber_count(), 0);

        let _s1 = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 1);
    }

    #[tokio::test]
    async fn subscribe_after_replays_newer_events() {
        let bus = SseBus::new(16).unwrap();
        let first = bus.publish(TestEvent { msg: "one".into() }).unwrap();
        let second = bus.publish(TestEvent { msg: "two".into() }).unwrap();

        let mut stream = std::pin::pin!(bus.subscribe_after(Some(&first.id)));
        let replayed = stream.next().await.expect("replayed").unwrap();
        assert_eq!(replayed.id, second.id);
    }

    #[test]
    fn capacity_above_maximum_is_rejected() {
        match SseBus::<TestEvent>::new(MAX_CAPACITY + 1) {
            Ok(_) => panic!("capacity above maximum should fail"),
            Err(error) => assert!(error.to_string().contains("at most")),
        }
    }

    #[test]
    fn event_to_axum_includes_optional_metadata() {
        let event = SseEvent {
            id: "42".to_string(),
            event: Some("message".to_string()),
            retry: Some(Duration::from_millis(250)),
            data: TestEvent { msg: "ok".into() },
        };

        assert!(event.into_axum_event().is_ok());
    }

    #[test]
    fn event_to_axum_reports_serialization_failures() {
        let event = SseEvent {
            id: "1".to_string(),
            event: None,
            retry: None,
            data: FailingEvent,
        };

        let err = event.into_axum_event().unwrap_err();
        assert_eq!(err.code(), ErrorCode::InvalidInput);
        assert!(err.to_string().contains("failed to serialize"));
    }

    #[tokio::test]
    async fn axum_stream_replays_and_skips_unserializable_events() {
        let bus = SseBus::new(4).unwrap().with_retry(Duration::from_secs(1));
        let first = bus.publish(TestEvent { msg: "one".into() }).unwrap();
        bus.publish(TestEvent { msg: "two".into() }).unwrap();

        let mut stream = std::pin::pin!(bus.subscribe_axum_after(Some(&first.id)));
        let event = stream.next().await.expect("axum replay").unwrap();
        drop(event);
    }

    #[tokio::test]
    async fn axum_live_stream_and_serialization_skip_paths_are_exercised() {
        let bus = SseBus::new(4).unwrap();
        let mut stream = std::pin::pin!(bus.subscribe_axum());
        bus.publish(TestEvent { msg: "live".into() }).unwrap();
        let event = stream.next().await.expect("live event").unwrap();
        drop(event);

        let skipped = axum_event(Ok(SseEvent {
            id: "bad".to_string(),
            event: None,
            retry: None,
            data: FailingEvent,
        }));
        assert!(skipped.is_none());
    }
}
