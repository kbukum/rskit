//! In-memory broker holding shared channel state and message history.

use std::sync::Arc;

use super::consumer::InMemoryConsumer;
use super::producer::InMemoryProducer;
use super::state::PublishState;
use crate::message::Message;

/// An in-memory message broker backed by a `tokio::sync::broadcast` channel.
///
/// Create one broker and hand out producers / consumers via [`InMemoryBroker::producer`]
/// and [`InMemoryBroker::consumer`].
///
/// Every message sent through the broker is recorded in an internal history
/// so that test assertion helpers can inspect what was published.
#[derive(Debug, Clone)]
pub struct InMemoryBroker<T: Clone + Send + Sync + 'static> {
    pub(super) state: Arc<PublishState<T>>,
}

impl<T: Clone + Send + Sync + 'static> InMemoryBroker<T> {
    /// Create a broker with the given channel capacity.
    pub fn new(capacity: usize) -> Self {
        let limit = capacity.max(1);
        Self {
            state: Arc::new(PublishState::new(limit, limit)),
        }
    }

    /// Create a producer attached to this broker.
    pub fn producer(&self) -> InMemoryProducer<T> {
        InMemoryProducer {
            state: self.state.clone(),
        }
    }

    /// Create a broker with explicit channel capacity and bounded history limit.
    ///
    /// The default [`InMemoryBroker::new`] bounds history by channel capacity.
    /// Use this constructor when tests need a different history limit.
    #[must_use]
    pub fn with_history_limit(capacity: usize, history_limit: usize) -> Self {
        Self {
            state: Arc::new(PublishState::new(capacity, history_limit)),
        }
    }

    /// Create a consumer attached to this broker.
    pub fn consumer(&self) -> InMemoryConsumer<T> {
        InMemoryConsumer::new(self.state.subscribe())
    }

    /// Return a clone of all messages published to `topic`.
    pub async fn messages(&self, topic: &str) -> Vec<Message<T>> {
        self.state.messages(topic).await
    }

    /// Return a clone of every message published to any topic.
    pub async fn all_messages(&self) -> Vec<Message<T>> {
        self.state.all_messages().await
    }

    /// Return the number of messages published to `topic`.
    pub async fn message_count(&self, topic: &str) -> usize {
        self.state.message_count(topic).await
    }

    /// Clear the recorded message history.
    pub async fn reset(&self) {
        self.state.reset().await;
    }

    /// Pre-create a topic so that it appears in [`InMemoryBroker::topic_names`].
    pub async fn create_topic(&self, topic: &str) {
        self.state.create_topic(topic).await;
    }

    /// Return the sorted set of topic names that have been created or published to.
    pub async fn topic_names(&self) -> Vec<String> {
        self.state.topic_names().await
    }

    /// Wait until the next publish wakes this task.
    pub(super) async fn notified(&self) {
        self.state.notified().await;
    }
}

impl<T: Clone + Send + Sync + 'static> Default for InMemoryBroker<T> {
    fn default() -> Self {
        Self::new(256)
    }
}
