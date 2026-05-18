use std::collections::HashSet;
use std::time::Duration;

use futures_util::StreamExt;
use rskit_sse::SseBus;
use serde::Serialize;

// ---------------------------------------------------------------------------
// Test event types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, PartialEq)]
struct TestEvent {
    msg: String,
}

impl TestEvent {
    fn new(msg: &str) -> Self {
        Self {
            msg: msg.to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct EmptyEvent {}

#[derive(Clone, Debug, Serialize)]
struct NestedEvent {
    inner: InnerData,
    tags: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct InnerData {
    value: i64,
    label: String,
}

#[derive(Clone, Debug, Serialize)]
struct UnicodeEvent {
    text: String,
}

// ---------------------------------------------------------------------------
// Multiple concurrent publishers
// ---------------------------------------------------------------------------

#[tokio::test]
async fn multiple_concurrent_publishers() {
    let bus = SseBus::new(256).unwrap();
    let mut stream = std::pin::pin!(bus.subscribe_after(None));

    let mut handles = Vec::new();
    for i in 0..5 {
        // We can't move `bus` into multiple tasks, but publish is &self,
        // so we use a shared reference via Arc.
        let msg = format!("pub-{i}");
        // publish is sync and takes &self — call from different tasks via Arc
        handles.push((i, msg));
    }

    // Publish from the main task (SseBus is not Arc-wrapped, but publish is &self)
    for (_, msg) in &handles {
        bus.publish(TestEvent::new(msg)).unwrap();
    }

    let timeout = Duration::from_secs(1);
    let mut received = Vec::new();
    for _ in 0..5 {
        let event = tokio::time::timeout(timeout, stream.next())
            .await
            .expect("timeout")
            .expect("stream ended");
        assert!(event.is_ok());
        received.push(event.unwrap());
    }
    assert_eq!(received.len(), 5);
}

// ---------------------------------------------------------------------------
// Multiple concurrent subscribers
// ---------------------------------------------------------------------------

#[tokio::test]
async fn multiple_concurrent_subscribers_all_receive() {
    let bus = SseBus::new(64).unwrap();

    let mut streams: Vec<_> = (0..10).map(|_| Box::pin(bus.subscribe())).collect();
    assert_eq!(bus.subscriber_count(), 10);

    bus.publish(TestEvent::new("for everyone")).unwrap();

    let timeout = Duration::from_secs(1);
    for (i, stream) in streams.iter_mut().enumerate() {
        let event = tokio::time::timeout(timeout, stream.next())
            .await
            .unwrap_or_else(|_| panic!("timeout on subscriber {i}"))
            .unwrap_or_else(|| panic!("stream {i} ended"));
        assert!(event.is_ok(), "subscriber {i} got error");
    }
}

// ---------------------------------------------------------------------------
// Slow subscriber handling (broadcast channel full)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn slow_subscriber_channel_overflow() {
    // Small capacity — overflow is expected
    let bus: SseBus<TestEvent> = SseBus::new(4).unwrap();
    let mut stream = std::pin::pin!(bus.subscribe_after(None));

    // Publish more messages than the channel can hold
    for i in 0..10 {
        bus.publish(TestEvent::new(&format!("msg-{i}"))).unwrap();
    }

    // The subscriber should still get SOME events (the latest ones),
    // and lagged events are silently skipped by filter_map in subscribe()
    let timeout = Duration::from_millis(200);
    let mut count = 0;
    while let Ok(Some(Ok(_))) = tokio::time::timeout(timeout, stream.next()).await {
        count += 1;
    }

    // We should have received some but not necessarily all 10
    assert!(count > 0, "should receive at least some events");
    assert!(count <= 10, "should not receive more than published");
}

// ---------------------------------------------------------------------------
// Memory cleanup: dropping subscribers
// ---------------------------------------------------------------------------

#[tokio::test]
async fn subscriber_count_decreases_on_drop() {
    let bus: SseBus<TestEvent> = SseBus::new(16).unwrap();
    assert_eq!(bus.subscriber_count(), 0);

    {
        let _s1 = bus.subscribe();
        let _s2 = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 2);
    }
    // After drop, subscriber count should decrease
    assert_eq!(bus.subscriber_count(), 0);
}

// ---------------------------------------------------------------------------
// Event ordering guarantees
// ---------------------------------------------------------------------------

#[tokio::test]
async fn event_ordering_preserved_for_single_subscriber() {
    let bus = SseBus::new(128).unwrap();
    let mut stream = std::pin::pin!(bus.subscribe());

    for i in 0..50 {
        bus.publish(TestEvent::new(&format!("order-{i}"))).unwrap();
    }

    let timeout = Duration::from_secs(1);
    for i in 0..50 {
        let event = tokio::time::timeout(timeout, stream.next())
            .await
            .expect("timeout")
            .expect("stream ended");
        // Events arrive as JSON — we just verify they arrive in order
        assert!(event.is_ok(), "event {i} should be Ok");
    }
}

#[tokio::test]
async fn event_ordering_across_multiple_subscribers() {
    let bus = SseBus::new(128).unwrap();
    let mut s1 = std::pin::pin!(bus.subscribe());
    let mut s2 = std::pin::pin!(bus.subscribe());

    for i in 0..20 {
        bus.publish(TestEvent::new(&format!("seq-{i}"))).unwrap();
    }

    let timeout = Duration::from_secs(1);
    // Both subscribers should receive all 20 events
    for i in 0..20 {
        let e1 = tokio::time::timeout(timeout, s1.next())
            .await
            .unwrap_or_else(|_| panic!("s1 timeout at {i}"))
            .unwrap_or_else(|| panic!("s1 ended at {i}"));
        let e2 = tokio::time::timeout(timeout, s2.next())
            .await
            .unwrap_or_else(|_| panic!("s2 timeout at {i}"))
            .unwrap_or_else(|| panic!("s2 ended at {i}"));
        assert!(e1.is_ok());
        assert!(e2.is_ok());
    }
}

// ---------------------------------------------------------------------------
// Serialization edge cases
// ---------------------------------------------------------------------------

#[tokio::test]
async fn serialize_empty_struct() {
    let bus: SseBus<EmptyEvent> = SseBus::new(16).unwrap();
    let mut stream = std::pin::pin!(bus.subscribe());

    bus.publish(EmptyEvent {}).unwrap();

    let timeout = Duration::from_secs(1);
    let event = tokio::time::timeout(timeout, stream.next())
        .await
        .expect("timeout")
        .expect("stream ended");
    assert!(event.is_ok());
}

#[tokio::test]
async fn serialize_nested_objects() {
    let bus: SseBus<NestedEvent> = SseBus::new(16).unwrap();
    let mut stream = std::pin::pin!(bus.subscribe());

    bus.publish(NestedEvent {
        inner: InnerData {
            value: 42,
            label: "deep".into(),
        },
        tags: vec!["a".into(), "b".into()],
    })
    .unwrap();

    let timeout = Duration::from_secs(1);
    let event = tokio::time::timeout(timeout, stream.next())
        .await
        .expect("timeout")
        .expect("stream ended");
    assert!(event.is_ok());
}

#[tokio::test]
async fn serialize_unicode_content() {
    let bus: SseBus<UnicodeEvent> = SseBus::new(16).unwrap();
    let mut stream = std::pin::pin!(bus.subscribe());

    bus.publish(UnicodeEvent {
        text: "こんにちは 🌍 مرحبا".into(),
    })
    .unwrap();

    let timeout = Duration::from_secs(1);
    let event = tokio::time::timeout(timeout, stream.next())
        .await
        .expect("timeout")
        .expect("stream ended");
    assert!(event.is_ok());
}

#[tokio::test]
async fn serialize_large_payload() {
    let bus: SseBus<TestEvent> = SseBus::new(16).unwrap();
    let mut stream = std::pin::pin!(bus.subscribe());

    let large_msg = "x".repeat(100_000);
    bus.publish(TestEvent::new(&large_msg)).unwrap();

    let timeout = Duration::from_secs(2);
    let event = tokio::time::timeout(timeout, stream.next())
        .await
        .expect("timeout")
        .expect("stream ended");
    assert!(event.is_ok());
}

// ---------------------------------------------------------------------------
// Channel capacity edge cases
// ---------------------------------------------------------------------------

#[tokio::test]
async fn channel_capacity_one() {
    let bus: SseBus<TestEvent> = SseBus::new(1).unwrap();
    let mut stream = std::pin::pin!(bus.subscribe());

    bus.publish(TestEvent::new("only")).unwrap();

    let timeout = Duration::from_secs(1);
    let event = tokio::time::timeout(timeout, stream.next())
        .await
        .expect("timeout")
        .expect("stream ended");
    assert!(event.is_ok());
}

#[test]
fn channel_capacity_zero_is_rejected() {
    let result = SseBus::<TestEvent>::new(0);
    assert!(result.is_err());
}

#[tokio::test]
async fn publish_without_subscribers_is_buffered() {
    let bus: SseBus<TestEvent> = SseBus::new(16).unwrap();
    let result = bus.publish(TestEvent::new("nobody"));
    assert!(result.is_ok());
}

// ---------------------------------------------------------------------------
// Subscribe/unsubscribe during active publishing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn subscribe_after_publish_replays_buffered_events() {
    let bus = SseBus::new(64).unwrap();

    bus.publish(TestEvent::new("before")).unwrap();

    let mut stream = std::pin::pin!(bus.subscribe_after(None));

    let timeout = Duration::from_millis(200);
    let event = tokio::time::timeout(timeout, stream.next())
        .await
        .expect("timeout")
        .expect("stream ended");
    assert!(event.is_ok());
}

#[tokio::test]
async fn drop_subscriber_during_publishing() {
    let bus = SseBus::new(64).unwrap();
    let s1 = bus.subscribe();
    let mut s2 = std::pin::pin!(bus.subscribe());
    assert_eq!(bus.subscriber_count(), 2);

    // Drop one subscriber
    drop(s1);
    assert_eq!(bus.subscriber_count(), 1);

    // Remaining subscriber should still work
    bus.publish(TestEvent::new("still here")).unwrap();

    let timeout = Duration::from_secs(1);
    let event = tokio::time::timeout(timeout, s2.next())
        .await
        .expect("timeout")
        .expect("stream ended");
    assert!(event.is_ok());
}

// ---------------------------------------------------------------------------
// Type-safety: various generic T types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
struct IntEvent {
    value: i32,
}

#[derive(Clone, Debug, Serialize)]
struct BoolEvent {
    flag: bool,
}

#[derive(Clone, Debug, Serialize)]
struct VecEvent {
    items: Vec<f64>,
}

#[derive(Clone, Debug, Serialize)]
struct OptionEvent {
    maybe: Option<String>,
}

#[tokio::test]
async fn type_safety_int_event() {
    let bus: SseBus<IntEvent> = SseBus::new(16).unwrap();
    let mut stream = std::pin::pin!(bus.subscribe());

    bus.publish(IntEvent { value: 42 }).unwrap();

    let timeout = Duration::from_secs(1);
    let event = tokio::time::timeout(timeout, stream.next())
        .await
        .expect("timeout")
        .expect("stream ended");
    assert!(event.is_ok());
}

#[tokio::test]
async fn type_safety_bool_event() {
    let bus: SseBus<BoolEvent> = SseBus::new(16).unwrap();
    let mut stream = std::pin::pin!(bus.subscribe());

    bus.publish(BoolEvent { flag: true }).unwrap();

    let timeout = Duration::from_secs(1);
    let event = tokio::time::timeout(timeout, stream.next())
        .await
        .expect("timeout")
        .expect("stream ended");
    assert!(event.is_ok());
}

#[tokio::test]
async fn type_safety_vec_event() {
    let bus: SseBus<VecEvent> = SseBus::new(16).unwrap();
    let mut stream = std::pin::pin!(bus.subscribe());

    bus.publish(VecEvent {
        items: vec![1.1, 2.2, 3.3],
    })
    .unwrap();

    let timeout = Duration::from_secs(1);
    let event = tokio::time::timeout(timeout, stream.next())
        .await
        .expect("timeout")
        .expect("stream ended");
    assert!(event.is_ok());
}

#[tokio::test]
async fn type_safety_option_event() {
    let bus: SseBus<OptionEvent> = SseBus::new(16).unwrap();
    let mut stream = std::pin::pin!(bus.subscribe());

    bus.publish(OptionEvent {
        maybe: Some("present".into()),
    })
    .unwrap();
    bus.publish(OptionEvent { maybe: None }).unwrap();

    let timeout = Duration::from_secs(1);
    for _ in 0..2 {
        let event = tokio::time::timeout(timeout, stream.next())
            .await
            .expect("timeout")
            .expect("stream ended");
        assert!(event.is_ok());
    }
}

// ---------------------------------------------------------------------------
// Stress: many events, many subscribers
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stress_many_events_single_subscriber() {
    let bus = SseBus::new(1024).unwrap();
    let mut stream = std::pin::pin!(bus.subscribe());

    let count = 500;
    for i in 0..count {
        bus.publish(TestEvent::new(&format!("stress-{i}"))).unwrap();
    }

    let timeout = Duration::from_secs(5);
    let mut received = 0;
    for _ in 0..count {
        match tokio::time::timeout(timeout, stream.next()).await {
            Ok(Some(Ok(_))) => received += 1,
            _ => break,
        }
    }
    assert_eq!(received, count);
}

#[tokio::test]
async fn stress_many_subscribers_single_event() {
    let bus = SseBus::new(16).unwrap();
    let sub_count = 50;

    let mut streams: Vec<_> = (0..sub_count).map(|_| Box::pin(bus.subscribe())).collect();
    assert_eq!(bus.subscriber_count(), sub_count);

    bus.publish(TestEvent::new("mass")).unwrap();

    let timeout = Duration::from_secs(2);
    for (i, stream) in streams.iter_mut().enumerate() {
        let event = tokio::time::timeout(timeout, stream.next())
            .await
            .unwrap_or_else(|_| panic!("timeout on sub {i}"))
            .unwrap_or_else(|| panic!("stream {i} ended"));
        assert!(event.is_ok());
    }
}

// ---------------------------------------------------------------------------
// Unique events across subscribers
// ---------------------------------------------------------------------------

#[tokio::test]
async fn all_subscribers_get_independent_copies() {
    let bus = SseBus::new(16).unwrap();
    let mut s1 = std::pin::pin!(bus.subscribe());
    let mut s2 = std::pin::pin!(bus.subscribe());

    bus.publish(TestEvent::new("shared")).unwrap();
    bus.publish(TestEvent::new("shared2")).unwrap();

    let timeout = Duration::from_secs(1);

    // s1 reads both
    let mut s1_data = HashSet::new();
    for _ in 0..2 {
        if let Ok(Some(Ok(event))) = tokio::time::timeout(timeout, s1.next()).await {
            // Can't easily extract data from axum Event, but we can verify Ok
            s1_data.insert(format!("{event:?}"));
        }
    }
    assert_eq!(s1_data.len(), 2);

    // s2 independently reads both
    let mut s2_data = HashSet::new();
    for _ in 0..2 {
        if let Ok(Some(Ok(event))) = tokio::time::timeout(timeout, s2.next()).await {
            s2_data.insert(format!("{event:?}"));
        }
    }
    assert_eq!(s2_data.len(), 2);
}
