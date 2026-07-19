use tokio::sync::{broadcast, oneshot};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use rskit_errors::AppResult;

use crate::event::Event;

/// Handle returned to a caller after submitting a task to the pool.
///
/// - `result()` awaits the final task output.
/// - `events()` returns a broadcast receiver for intermediate [`Event`]s.
/// - `cancel()` requests cooperative cancellation.
pub struct TaskHandle<O: Clone + Send + 'static> {
    /// Unique identifier assigned to this task by the pool.
    pub id: Uuid,
    events_rx: broadcast::Receiver<Event<O>>,
    result_rx: oneshot::Receiver<AppResult<O>>,
    cancel: CancellationToken,
}

impl<O: Clone + Send + 'static> TaskHandle<O> {
    pub(crate) fn new(
        id: Uuid,
        events_rx: broadcast::Receiver<Event<O>>,
        result_rx: oneshot::Receiver<AppResult<O>>,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            id,
            events_rx,
            result_rx,
            cancel,
        }
    }

    /// Await the final result of the task.
    pub async fn result(self) -> AppResult<O> {
        match self.result_rx.await {
            Ok(r) => r,
            Err(_) => Err(rskit_errors::AppError::new(
                rskit_errors::ErrorCode::Internal,
                "worker task dropped before completing",
            )),
        }
    }

    /// Get a new broadcast receiver for intermediate events.
    pub fn events(&self) -> broadcast::Receiver<Event<O>> {
        self.events_rx.resubscribe()
    }

    /// Signal the task to cancel.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// Clone the cancellation token
    /// so it can be stored separately (e.g., for cancelling after the handle is consumed by `result()`).
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::{broadcast, oneshot};

    use super::*;

    #[tokio::test]
    async fn dropped_result_sender_maps_to_internal_error() {
        let (events_tx, events_rx) = broadcast::channel(1);
        let (result_tx, result_rx) = oneshot::channel();
        drop(events_tx);
        drop(result_tx);
        let handle = TaskHandle::<u32>::new(
            Uuid::new_v4(),
            events_rx,
            result_rx,
            CancellationToken::new(),
        );

        let error = handle.result().await.unwrap_err();

        assert_eq!(error.code(), rskit_errors::ErrorCode::Internal);
    }
}
