//! Test assertion and wait helpers over an [`InMemoryBroker`].

use std::time::Duration;

use super::broker::InMemoryBroker;
use crate::message::Message;

/// Assert that at least one message on `topic` satisfies the predicate.
///
/// # Panics
///
/// Panics when no matching message is found.
pub async fn assert_published<T: Clone + Send + Sync + 'static>(
    broker: &InMemoryBroker<T>,
    topic: &str,
    predicate: impl Fn(&Message<T>) -> bool,
) {
    let msgs = broker.messages(topic).await;
    assert!(
        msgs.iter().any(&predicate),
        "assert_published: no message on topic {topic:?} matched the predicate ({} checked)",
        msgs.len(),
    );
}

/// Assert that exactly `n` messages were published to `topic`.
///
/// # Panics
///
/// Panics when the count does not match.
pub async fn assert_published_n<T: Clone + Send + Sync + 'static>(
    broker: &InMemoryBroker<T>,
    topic: &str,
    n: usize,
) {
    let got = broker.message_count(topic).await;
    assert_eq!(
        got, n,
        "assert_published_n: topic {topic:?} has {got} messages, want {n}",
    );
}

/// Wait until at least one message appears on `topic` or the timeout expires.
///
/// Returns the first message on the topic.
///
/// # Panics
///
/// Panics if the timeout elapses before any message arrives.
pub async fn wait_for_message<T: Clone + Send + Sync + 'static>(
    broker: &InMemoryBroker<T>,
    topic: &str,
    timeout: Duration,
) -> Message<T> {
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        let msgs = broker.messages(topic).await;
        if let Some(m) = msgs.into_iter().next() {
            return m;
        }
        tokio::select! {
            () = broker.notified() => { /* re-check */ }
            () = tokio::time::sleep_until(deadline) => {
                panic!("wait_for_message: timed out after {timeout:?} waiting for message on topic {topic:?}");
            }
        }
    }
}

/// Assert that zero messages were published to `topic`.
///
/// # Panics
///
/// Panics when the topic is not empty.
pub async fn assert_no_messages<T: Clone + Send + Sync + 'static>(
    broker: &InMemoryBroker<T>,
    topic: &str,
) {
    let n = broker.message_count(topic).await;
    assert_eq!(
        n, 0,
        "assert_no_messages: topic {topic:?} has {n} messages, want 0",
    );
}
