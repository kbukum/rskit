//! Bounded async worker pool with streaming events and cooperative cancellation.

#![warn(missing_docs)]

/// Provider/handler bridge adapters for interoperability with `rskit-provider`.
pub mod bridge;
/// Task dispatch strategies (e.g. round-robin).
pub mod dispatch;
/// Event types emitted by tasks during execution.
pub mod event;
/// [`Handler`] trait implemented by task executors.
pub mod handler;
/// [`Pool`] and [`PoolConfig`] for managing concurrent task execution.
pub mod pool;
/// [`TaskHandle`] returned to callers after task submission.
pub mod task;

pub use bridge::{as_provider, from_provider};
pub use dispatch::{DispatchStrategy, RoundRobinDispatcher};
pub use event::{Event, EventKind, Progress};
pub use handler::Handler;
pub use pool::{OverflowPolicy, Pool, PoolConfig, PoolStats};
pub use task::TaskHandle;

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    use rskit_errors::{AppError, AppResult, ErrorCode};
    use rskit_provider::request_response_fn;
    use rskit_provider::traits::RequestResponse;

    use crate::bridge::{as_provider, from_provider};
    use crate::event::{Event, Progress};
    use crate::handler::Handler;
    use crate::pool::{Pool, PoolConfig};

    // ── Shared test handler: doubles its input ────────────────────────────────

    struct DoubleHandler;

    #[async_trait::async_trait]
    impl Handler<i32, i32> for DoubleHandler {
        async fn handle(
            &self,
            task: i32,
            _emit: mpsc::Sender<Event<i32>>,
            _cancel: CancellationToken,
        ) -> AppResult<i32> {
            Ok(task * 2)
        }
    }

    // ── Handler that always errors ────────────────────────────────────────────

    struct ErrorHandler;

    #[async_trait::async_trait]
    impl Handler<i32, i32> for ErrorHandler {
        async fn handle(
            &self,
            _task: i32,
            _emit: mpsc::Sender<Event<i32>>,
            _cancel: CancellationToken,
        ) -> AppResult<i32> {
            Err(AppError::new(ErrorCode::Internal, "boom"))
        }
    }

    // ── Handler that emits one Progress event ─────────────────────────────────

    struct ProgressHandler;

    #[async_trait::async_trait]
    impl Handler<i32, i32> for ProgressHandler {
        async fn handle(
            &self,
            task: i32,
            emit: mpsc::Sender<Event<i32>>,
            _cancel: CancellationToken,
        ) -> AppResult<i32> {
            // Use a placeholder UUID; the pool assigns the real task_id, so here
            // we just need any valid UUID for the emitted event.
            let fake_id = uuid::Uuid::new_v4();
            let p = Progress::new(1, Some(1));
            let ev = Event::progress(fake_id, "progress-handler", p);
            let _ = emit.send(ev).await;
            Ok(task * 2)
        }
    }

    // ── Handler that adds 1 ───────────────────────────────────────────────────

    struct PlusOneHandler;

    #[async_trait::async_trait]
    impl Handler<i32, i32> for PlusOneHandler {
        async fn handle(
            &self,
            task: i32,
            _emit: mpsc::Sender<Event<i32>>,
            _cancel: CancellationToken,
        ) -> AppResult<i32> {
            Ok(task + 1)
        }
    }

    struct BlockingHandler {
        started: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
    }

    #[async_trait::async_trait]
    impl Handler<i32, i32> for BlockingHandler {
        async fn handle(
            &self,
            task: i32,
            _emit: mpsc::Sender<Event<i32>>,
            _cancel: CancellationToken,
        ) -> AppResult<i32> {
            self.started.notify_one();
            self.release.notified().await;
            Ok(task)
        }
    }

    // ── 1. pool_submit_and_await_result ───────────────────────────────────────

    #[tokio::test]
    async fn pool_submit_and_await_result() {
        let pool = Pool::new(Arc::new(DoubleHandler), PoolConfig::new("test-pool"));
        let handle = pool.submit(21).await.unwrap();
        let result = handle.result().await;
        assert_eq!(result.unwrap(), 42);
    }

    // ── 2. pool_submit_error_propagates ──────────────────────────────────────

    #[tokio::test]
    async fn pool_submit_error_propagates() {
        let pool = Pool::new(Arc::new(ErrorHandler), PoolConfig::new("error-pool"));
        let handle = pool.submit(1).await.unwrap();
        let result = handle.result().await;
        assert!(result.is_err(), "expected Err but got {result:?}");
    }

    // ── 3. pool_task_handle_events ────────────────────────────────────────────

    #[tokio::test]
    async fn pool_task_handle_events() {
        let pool = Pool::new(Arc::new(ProgressHandler), PoolConfig::new("event-pool"));
        let handle = pool.submit(5).await.unwrap();
        let task_id = handle.id;
        let mut event_rx = handle.events();

        // Await the final result so the task has fully run
        let result = handle.result().await;
        assert!(result.is_ok());

        // Collect all events received (progress + the auto-emitted Result event)
        let mut received = Vec::new();
        while let Ok(ev) = event_rx.try_recv() {
            received.push(ev);
        }

        // The pool always emits at least a final Result event with the correct task_id
        let has_result_event = received.iter().any(|ev| ev.task_id == task_id);
        assert!(
            has_result_event,
            "expected at least one event with task_id {task_id}, got: {received:?}"
        );
    }

    // ── 4. from_provider_bridge ───────────────────────────────────────────────

    #[tokio::test]
    async fn from_provider_bridge() {
        let provider = Arc::new(request_response_fn("p", |x: i32| async move { Ok(x + 1) }));
        let handler = from_provider(provider);
        let pool = Pool::new(Arc::new(handler), PoolConfig::new("bridge-pool"));
        let handle = pool.submit(9).await.unwrap();
        let result = handle.result().await;
        assert_eq!(result.unwrap(), 10);
    }

    // ── 5. as_provider_bridge ─────────────────────────────────────────────────

    #[tokio::test]
    async fn as_provider_bridge() {
        let handler: Arc<dyn Handler<i32, i32>> = Arc::new(PlusOneHandler);
        let provider = as_provider("name", handler);
        let result = provider.execute(5).await;
        assert_eq!(result.unwrap(), 6);
    }

    #[tokio::test]
    async fn reject_overflow_policy_rejects_new_submission() {
        let started = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let pool = Pool::new(
            Arc::new(BlockingHandler {
                started: started.clone(),
                release: release.clone(),
            }),
            PoolConfig::new("reject-pool")
                .with_size(1)
                .with_queue_size(1)
                .with_overflow_policy(crate::OverflowPolicy::Reject),
        );

        // Task 1 starts processing, holding the only worker slot.
        let _first = pool.submit(1).await.unwrap();
        started.notified().await;

        // Task 2 fills the queue (capacity 1).
        let _second = pool.submit(2).await.unwrap();

        // Task 3 should be rejected: queue full.
        let third = pool.submit(3).await;
        assert!(third.is_err());

        // Release task 1 so task 2 can run.
        release.notify_waiters();
        // Wait for task 2 to start, then release it.
        started.notified().await;
        release.notify_waiters();
    }

    #[tokio::test]
    async fn drop_oldest_overflow_policy_drops_queued_task() {
        let started = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let pool = Pool::new(
            Arc::new(BlockingHandler {
                started: started.clone(),
                release: release.clone(),
            }),
            PoolConfig::new("drop-oldest-pool")
                .with_size(1)
                .with_queue_size(1)
                .with_overflow_policy(crate::OverflowPolicy::DropOldest),
        );

        // Task 1 starts processing, holding the only worker slot.
        let _first = pool.submit(1).await.unwrap();
        started.notified().await;

        // Task 2 fills the queue.
        let second = pool.submit(2).await.unwrap();

        // Task 3 evicts task 2 (drop oldest).
        let _third = pool.submit(3).await.unwrap();

        // Task 2 should have been dropped with a RateLimited error.
        let dropped_error = second.result().await.unwrap_err();
        assert_eq!(dropped_error.code, ErrorCode::RateLimited);

        // Release task 1 so task 3 can run.
        release.notify_waiters();
        started.notified().await;
        release.notify_waiters();
    }
}
