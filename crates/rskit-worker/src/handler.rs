use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use rskit_errors::AppResult;

use crate::event::Event;

/// Core trait for worker task handlers.
///
/// Implementors receive a task `I`, an event sender for streaming intermediate results,
/// and a cancellation token for cooperative cancellation.
#[async_trait::async_trait]
pub trait Handler<I, O>: Send + Sync
where
    I: Send + 'static,
    O: Send + Clone + 'static,
{
    async fn handle(
        &self,
        task: I,
        emit: mpsc::Sender<Event<O>>,
        cancel: CancellationToken,
    ) -> AppResult<O>;
}
