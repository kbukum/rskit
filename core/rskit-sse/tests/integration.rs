use futures_util::StreamExt;
use rskit_sse::SseBus;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
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

#[tokio::test]
async fn create_sse_bus() {
    let bus: SseBus<TestEvent> = SseBus::new(16).unwrap();
    assert_eq!(bus.subscriber_count(), 0);
}

#[tokio::test]
async fn subscribe_returns_stream() {
    let bus: SseBus<TestEvent> = SseBus::new(16).unwrap();
    let _stream = bus.subscribe();
    assert_eq!(bus.subscriber_count(), 1);
}

#[tokio::test]
async fn publish_event_reaches_subscriber() {
    let bus = SseBus::new(16).unwrap();
    let mut stream = std::pin::pin!(bus.subscribe());

    bus.publish(TestEvent::new("hello")).unwrap();

    let event = tokio::time::timeout(std::time::Duration::from_secs(1), stream.next())
        .await
        .expect("timeout")
        .expect("stream ended");

    assert!(event.is_ok());
}

#[tokio::test]
async fn multiple_subscribers_receive_same_event() {
    let bus = SseBus::new(16).unwrap();
    let mut s1 = std::pin::pin!(bus.subscribe());
    let mut s2 = std::pin::pin!(bus.subscribe());
    assert_eq!(bus.subscriber_count(), 2);

    bus.publish(TestEvent::new("broadcast")).unwrap();

    let timeout = std::time::Duration::from_secs(1);

    let e1 = tokio::time::timeout(timeout, s1.next())
        .await
        .expect("timeout s1")
        .expect("stream s1 ended");
    let e2 = tokio::time::timeout(timeout, s2.next())
        .await
        .expect("timeout s2")
        .expect("stream s2 ended");

    assert!(e1.is_ok());
    assert!(e2.is_ok());
}

#[tokio::test]
async fn subscriber_count_updates_correctly() {
    let bus: SseBus<TestEvent> = SseBus::new(16).unwrap();
    assert_eq!(bus.subscriber_count(), 0);

    let _s1 = bus.subscribe();
    assert_eq!(bus.subscriber_count(), 1);

    let _s2 = bus.subscribe();
    assert_eq!(bus.subscriber_count(), 2);
}

#[tokio::test]
async fn publish_without_subscribers_is_buffered() {
    let bus: SseBus<TestEvent> = SseBus::new(16).unwrap();
    let result = bus.publish(TestEvent::new("nobody listening"));
    assert!(result.is_ok());
}

#[tokio::test]
async fn multiple_events_arrive_in_order() {
    let bus = SseBus::new(16).unwrap();
    let mut stream = std::pin::pin!(bus.subscribe());

    bus.publish(TestEvent::new("first")).unwrap();
    bus.publish(TestEvent::new("second")).unwrap();
    bus.publish(TestEvent::new("third")).unwrap();

    let timeout = std::time::Duration::from_secs(1);
    for _ in 0..3 {
        let event = tokio::time::timeout(timeout, stream.next())
            .await
            .expect("timeout")
            .expect("stream ended");
        assert!(event.is_ok());
    }
}
