//! Shared publish-side state cloned between an [`InMemoryBroker`](super::InMemoryBroker) and the
//! producers it hands out.

use std::collections::{HashSet, VecDeque};

use rskit_errors::{AppError, AppResult, ErrorCode};
use tokio::sync::{Mutex, broadcast};

use crate::message::Message;

/// The publish channel shared by a broker and every producer it creates.
///
/// Holds the broadcast sender, recorded history, topic set, and publish
/// notification together, so handing out a producer clones one `Arc` instead of
/// copying five separate fields.
#[derive(Debug)]
pub(super) struct PublishState<T: Clone + Send + Sync + 'static> {
    tx: broadcast::Sender<Message<T>>,
    history: Mutex<VecDeque<Message<T>>>,
    history_limit: Option<usize>,
    topics: Mutex<HashSet<String>>,
    notify: tokio::sync::Notify,
}

impl<T: Clone + Send + Sync + 'static> PublishState<T> {
    /// Create publish state with the given channel capacity and history limit.
    pub(super) fn new(capacity: usize, history_limit: usize) -> Self {
        let capacity = capacity.max(1);
        let limit = history_limit.max(1);
        let (tx, _) = broadcast::channel(capacity);
        Self {
            tx,
            history: Mutex::new(VecDeque::with_capacity(limit)),
            history_limit: Some(limit),
            topics: Mutex::new(HashSet::new()),
            notify: tokio::sync::Notify::new(),
        }
    }

    /// Subscribe a fresh receiver to the broadcast channel.
    pub(super) fn subscribe(&self) -> broadcast::Receiver<Message<T>> {
        self.tx.subscribe()
    }

    /// Record a message in history, register its topic, broadcast it, and wake waiters.
    pub(super) async fn publish(&self, msg: Message<T>) -> AppResult<()> {
        {
            let mut hist = self.history.lock().await;
            if let Some(limit) = self.history_limit
                && hist.len() == limit
            {
                hist.pop_front();
            }
            hist.push_back(msg.clone());
        }
        self.topics.lock().await.insert(msg.topic.clone());

        self.tx.send(msg).map_err(|_| {
            AppError::new(ErrorCode::ExternalService, "no active consumers on channel")
        })?;

        self.notify.notify_waiters();
        Ok(())
    }

    /// Wait until the next publish wakes this task.
    pub(super) async fn notified(&self) {
        self.notify.notified().await;
    }

    /// Return a clone of every message published to `topic`.
    pub(super) async fn messages(&self, topic: &str) -> Vec<Message<T>> {
        self.history
            .lock()
            .await
            .iter()
            .filter(|m| m.topic == topic)
            .cloned()
            .collect()
    }

    /// Return a clone of every message published to any topic.
    pub(super) async fn all_messages(&self) -> Vec<Message<T>> {
        self.history.lock().await.iter().cloned().collect()
    }

    /// Return the number of messages published to `topic`.
    pub(super) async fn message_count(&self, topic: &str) -> usize {
        self.history
            .lock()
            .await
            .iter()
            .filter(|m| m.topic == topic)
            .count()
    }

    /// Clear the recorded message history.
    pub(super) async fn reset(&self) {
        self.history.lock().await.clear();
    }

    /// Pre-register a topic without publishing to it.
    pub(super) async fn create_topic(&self, topic: &str) {
        self.topics.lock().await.insert(topic.to_string());
    }

    /// Return the sorted set of topic names created or published to.
    pub(super) async fn topic_names(&self) -> Vec<String> {
        let mut set: HashSet<String> = self.topics.lock().await.clone();
        {
            let hist = self.history.lock().await;
            for m in hist.iter() {
                set.insert(m.topic.clone());
            }
        }
        let mut out: Vec<String> = set.into_iter().collect();
        out.sort();
        out
    }
}
