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
}
