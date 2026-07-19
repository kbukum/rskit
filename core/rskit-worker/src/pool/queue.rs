//! Bounded async submit queue backing the worker pool.

use std::collections::VecDeque;
use std::sync::Arc;

use parking_lot::Mutex;
use tokio::sync::Notify;

struct QueueInner<T> {
    items: VecDeque<T>,
    capacity: usize,
    closed: bool,
}

struct QueueState<T> {
    inner: Mutex<QueueInner<T>>,
    not_empty: Notify,
    not_full: Notify,
}

pub(super) struct SubmitQueue<T> {
    state: Arc<QueueState<T>>,
}

pub(super) enum PushRejectError<T> {
    Closed(T),
    Full(T),
}

pub(super) struct QueueReceiver<T> {
    state: Arc<QueueState<T>>,
}

impl<T> SubmitQueue<T> {
    pub(super) fn new(capacity: usize) -> (Self, QueueReceiver<T>) {
        let state = Arc::new(QueueState {
            inner: Mutex::new(QueueInner {
                items: VecDeque::with_capacity(capacity.max(1)),
                capacity: capacity.max(1),
                closed: false,
            }),
            not_empty: Notify::new(),
            not_full: Notify::new(),
        });
        (
            Self {
                state: Arc::clone(&state),
            },
            QueueReceiver { state },
        )
    }

    pub(super) async fn push_block(&self, item: T) -> Result<(), T> {
        loop {
            let notified = {
                let mut inner = self.state.inner.lock();
                if inner.closed {
                    return Err(item);
                }
                if inner.items.len() < inner.capacity {
                    inner.items.push_back(item);
                    self.state.not_empty.notify_one();
                    return Ok(());
                }
                self.state.not_full.notified()
            };
            notified.await;
        }
    }

    pub(super) fn push_reject(&self, item: T) -> Result<(), PushRejectError<T>> {
        let mut inner = self.state.inner.lock();
        if inner.closed {
            return Err(PushRejectError::Closed(item));
        }
        if inner.items.len() >= inner.capacity {
            return Err(PushRejectError::Full(item));
        }
        inner.items.push_back(item);
        self.state.not_empty.notify_one();
        Ok(())
    }

    pub(super) fn push_drop_oldest(&self, item: T) -> Result<Option<T>, T> {
        let mut inner = self.state.inner.lock();
        if inner.closed {
            return Err(item);
        }
        let dropped = if inner.items.len() >= inner.capacity {
            inner.items.pop_front()
        } else {
            None
        };
        inner.items.push_back(item);
        self.state.not_empty.notify_one();
        Ok(dropped)
    }

    pub(super) fn close(&self) {
        let mut inner = self.state.inner.lock();
        inner.closed = true;
        self.state.not_empty.notify_waiters();
        self.state.not_full.notify_waiters();
    }
}

impl<T> Clone for SubmitQueue<T> {
    fn clone(&self) -> Self {
        Self {
            state: Arc::clone(&self.state),
        }
    }
}

impl<T> QueueReceiver<T> {
    pub(super) async fn recv(&self) -> Option<T> {
        loop {
            let notified = {
                let mut inner = self.state.inner.lock();
                if let Some(item) = inner.items.pop_front() {
                    self.state.not_full.notify_one();
                    return Some(item);
                }
                if inner.closed {
                    return None;
                }
                self.state.not_empty.notified()
            };
            notified.await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn queue_rejects_when_full_and_closed_and_receives_until_closed() {
        let (queue, receiver) = SubmitQueue::new(1);
        let cloned = queue.clone();

        assert!(cloned.push_reject(1).is_ok());
        assert!(matches!(
            queue.push_reject(2),
            Err(PushRejectError::Full(2))
        ));
        assert_eq!(receiver.recv().await, Some(1));
        queue.close();
        assert!(matches!(
            queue.push_reject(3),
            Err(PushRejectError::Closed(3))
        ));
        assert_eq!(receiver.recv().await, None);
        assert_eq!(queue.push_drop_oldest(4), Err(4));
        assert_eq!(queue.push_block(5).await, Err(5));
    }
}
