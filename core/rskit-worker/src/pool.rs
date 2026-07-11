use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tokio::sync::{Notify, Semaphore, broadcast, mpsc, oneshot};
use tokio::task::{JoinHandle, JoinSet};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::dispatch::DispatchStrategy;
use crate::event::Event;
use crate::handler::Handler;
use crate::task::TaskHandle;

/// Statistics snapshot for the pool.
#[derive(Debug, Clone)]
pub struct PoolStats {
    /// Human-readable name of the pool.
    pub name: String,
    /// Number of tasks currently executing.
    pub running: usize,
    /// Maximum concurrent tasks the pool allows.
    pub capacity: usize,
}

/// Overflow behavior applied when the submission queue is full.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum OverflowPolicy {
    /// Wait until queue capacity becomes available.
    #[default]
    Block,
    /// Reject the new submission immediately.
    Reject,
    /// Drop the oldest queued task and enqueue the new submission.
    DropOldest,
}

/// Configuration for a [`Pool`].
pub struct PoolConfig {
    /// Human-readable name used in tracing.
    pub name: String,
    /// Maximum concurrent tasks (semaphore permits).
    pub size: usize,
    /// Capacity of the internal submit queue.
    pub queue_size: usize,
    /// Broadcast channel capacity for events per task.
    pub event_buffer: usize,
    /// Grace period given to in-flight tasks on shutdown.
    pub grace_period: Duration,
    /// Dispatch strategy (reserved for future multi-queue extensions).
    pub dispatch: DispatchStrategy,
    /// Queue overflow behavior.
    pub overflow_policy: OverflowPolicy,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            name: "pool".into(),
            size: available_parallelism(),
            queue_size: 256,
            event_buffer: 64,
            grace_period: Duration::from_secs(30),
            dispatch: DispatchStrategy::RoundRobin,
            overflow_policy: OverflowPolicy::Block,
        }
    }
}

impl PoolConfig {
    /// Create a named pool configuration with sensible defaults.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    /// Set the maximum number of concurrent tasks. Values below 1 are clamped
    /// to 1 inside `Pool::new` (with a tracing warning), since a zero-sized
    /// pool can never execute tasks.
    #[must_use]
    pub fn with_size(mut self, size: usize) -> Self {
        self.size = size;
        self
    }

    /// Set the capacity of the internal submit queue.
    #[must_use]
    pub fn with_queue_size(mut self, queue_size: usize) -> Self {
        self.queue_size = queue_size;
        self
    }

    /// Set the grace period given to in-flight tasks during shutdown.
    #[must_use]
    pub fn with_grace_period(mut self, d: Duration) -> Self {
        self.grace_period = d;
        self
    }

    /// Set the queue overflow policy.
    #[must_use]
    pub fn with_overflow_policy(mut self, overflow_policy: OverflowPolicy) -> Self {
        self.overflow_policy = overflow_policy;
        self
    }
}

fn available_parallelism() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

struct Envelope<I, O: Clone + Send + 'static> {
    id: Uuid,
    input: I,
    events_bcast: broadcast::Sender<Event<O>>,
    result_tx: oneshot::Sender<AppResult<O>>,
    cancel: CancellationToken,
    event_buffer: usize,
}

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

struct SubmitQueue<T> {
    state: Arc<QueueState<T>>,
}

enum PushRejectError<T> {
    Closed(T),
    Full(T),
}

struct QueueReceiver<T> {
    state: Arc<QueueState<T>>,
}

impl<T> SubmitQueue<T> {
    fn new(capacity: usize) -> (Self, QueueReceiver<T>) {
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

    async fn push_block(&self, item: T) -> Result<(), T> {
        let mut item = Some(item);
        loop {
            let notified = {
                let mut inner = self.state.inner.lock();
                if inner.closed {
                    return Err(item.take().unwrap_or_else(|| unreachable!("item present")));
                }
                if inner.items.len() < inner.capacity {
                    inner
                        .items
                        .push_back(item.take().unwrap_or_else(|| unreachable!("item present")));
                    self.state.not_empty.notify_one();
                    return Ok(());
                }
                self.state.not_full.notified()
            };
            notified.await;
        }
    }

    fn push_reject(&self, item: T) -> Result<(), PushRejectError<T>> {
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

    fn push_drop_oldest(&self, item: T) -> Result<Option<T>, T> {
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

    fn close(&self) {
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
    async fn recv(&self) -> Option<T> {
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

/// A bounded async worker pool.
pub struct Pool<I, O>
where
    I: Send + 'static,
    O: Send + Clone + 'static,
{
    name: String,
    queue: SubmitQueue<Envelope<I, O>>,
    semaphore: Arc<Semaphore>,
    capacity: usize,
    event_buffer: usize,
    overflow_policy: OverflowPolicy,
    grace_period: Duration,
    shutdown: CancellationToken,
    runner: Option<JoinHandle<()>>,
}

impl<I, O> Pool<I, O>
where
    I: Send + 'static,
    O: Send + Clone + 'static,
{
    /// Create a new pool backed by `handler`.
    ///
    /// `config.size` is clamped to a minimum of 1 (a zero-sized pool can never
    /// execute tasks because no permits would be available); a `tracing` warn
    /// is emitted when this clamp engages.
    pub fn new(handler: Arc<dyn Handler<I, O>>, config: PoolConfig) -> Self {
        let size = if config.size == 0 {
            tracing::warn!(
                pool = %config.name,
                "PoolConfig::size was 0, clamping to 1; a zero-sized pool can never execute tasks"
            );
            1
        } else {
            config.size
        };
        let semaphore = Arc::new(Semaphore::new(size));
        let (queue, receiver) = SubmitQueue::<Envelope<I, O>>::new(config.queue_size);
        let shutdown = CancellationToken::new();

        let runner = tokio::spawn(runner_loop(
            config.name.clone(),
            handler,
            receiver,
            semaphore.clone(),
            shutdown.clone(),
        ));

        Pool {
            name: config.name,
            queue,
            semaphore,
            capacity: size,
            event_buffer: config.event_buffer,
            overflow_policy: config.overflow_policy,
            grace_period: config.grace_period,
            shutdown,
            runner: Some(runner),
        }
    }

    /// Submit a task; returns a [`TaskHandle`] immediately.
    pub async fn submit(&self, input: I) -> AppResult<TaskHandle<O>> {
        let id = Uuid::new_v4();
        let (bcast_tx, bcast_rx) = broadcast::channel::<Event<O>>(self.event_buffer.max(1));
        let (result_tx, result_rx) = oneshot::channel::<AppResult<O>>();
        let cancel = CancellationToken::new();

        let handle = TaskHandle::new(id, bcast_rx, result_rx, cancel.clone());
        let envelope = Envelope {
            id,
            input,
            events_bcast: bcast_tx,
            result_tx,
            cancel,
            event_buffer: self.event_buffer.max(1),
        };

        match self.overflow_policy {
            OverflowPolicy::Block => {
                self.queue.push_block(envelope).await.map_err(|_| {
                    AppError::new(
                        ErrorCode::ServiceUnavailable,
                        format!("pool '{}' is shut down", self.name),
                    )
                })?;
            }
            OverflowPolicy::Reject => {
                self.queue.push_reject(envelope).map_err(|err| match err {
                    PushRejectError::Closed(_) => AppError::new(
                        ErrorCode::ServiceUnavailable,
                        format!("pool '{}' is shut down", self.name),
                    ),
                    PushRejectError::Full(_) => AppError::rate_limited()
                        .with_detail("pool", self.name.clone())
                        .with_detail("overflow_policy", "reject"),
                })?;
            }
            OverflowPolicy::DropOldest => {
                let dropped = self.queue.push_drop_oldest(envelope).map_err(|_| {
                    AppError::new(
                        ErrorCode::ServiceUnavailable,
                        format!("pool '{}' is shut down", self.name),
                    )
                })?;
                if let Some(dropped) = dropped {
                    notify_dropped_task(dropped, &self.name);
                }
            }
        }

        Ok(handle)
    }

    /// Snapshot of pool activity.
    pub fn stats(&self) -> PoolStats {
        let running = self
            .capacity
            .saturating_sub(self.semaphore.available_permits());
        PoolStats {
            name: self.name.clone(),
            running,
            capacity: self.capacity,
        }
    }

    /// Number of permits currently available for task execution.
    #[must_use]
    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }

    /// Stop accepting work and ask the runner loop to exit.
    pub fn close(&self) {
        self.shutdown.cancel();
        self.queue.close();
    }

    /// Cancel all in-flight tasks and shut down the runner loop.
    pub async fn shutdown(mut self) -> AppResult<()> {
        self.close();
        if let Some(runner) = self.runner.take() {
            let mut runner = runner;
            let wait = tokio::time::timeout(self.grace_period, &mut runner).await;
            match wait {
                Ok(joined) => joined.map_err(|err| {
                    AppError::new(
                        ErrorCode::Internal,
                        format!("pool '{}' runner failed during shutdown: {err}", self.name),
                    )
                })?,
                Err(_) => {
                    tracing::warn!(
                        pool = %self.name,
                        grace_period_ms = self.grace_period.as_millis(),
                        "shutdown grace period elapsed; aborting runner"
                    );
                    self.shutdown.cancel();
                    runner.abort();
                    let _ = runner.await;
                }
            }
        }
        Ok(())
    }
}

impl<I, O> Drop for Pool<I, O>
where
    I: Send + 'static,
    O: Send + Clone + 'static,
{
    fn drop(&mut self) {
        self.close();
        if let Some(runner) = self.runner.take() {
            runner.abort();
        }
    }
}

fn notify_dropped_task<I, O>(envelope: Envelope<I, O>, pool_name: &str)
where
    O: Clone + Send + 'static,
{
    let error = AppError::rate_limited()
        .with_detail("pool", pool_name.to_string())
        .with_detail("overflow_policy", "drop_oldest");
    let _ = envelope.events_bcast.send(Event::error(
        envelope.id,
        format!("{pool_name}/queue"),
        error.message().to_string(),
    ));
    let _ = envelope.result_tx.send(Err(error));
}

/// Complete a dequeued envelope with a `ServiceUnavailable` error when the
/// pool is shutting down before the task could be dispatched. Without this
/// the envelope's `result_tx` would simply be dropped, leaving any awaiting
/// `TaskHandle::result()` to surface the resulting `RecvError` as a generic
/// channel-closed error rather than a meaningful "pool is shutting down".
fn fail_envelope_shutdown<I, O>(envelope: Envelope<I, O>, pool_name: &str)
where
    O: Clone + Send + 'static,
{
    let error = AppError::new(
        ErrorCode::ServiceUnavailable,
        format!("pool '{pool_name}' is shutting down"),
    );
    let _ = envelope.events_bcast.send(Event::error(
        envelope.id,
        format!("{pool_name}/shutdown"),
        error.message().to_string(),
    ));
    let _ = envelope.result_tx.send(Err(error));
}

async fn runner_loop<I, O>(
    pool_name: String,
    handler: Arc<dyn Handler<I, O>>,
    receiver: QueueReceiver<Envelope<I, O>>,
    semaphore: Arc<Semaphore>,
    shutdown: CancellationToken,
) where
    I: Send + 'static,
    O: Send + Clone + 'static,
{
    let mut join_set: JoinSet<()> = JoinSet::new();

    loop {
        let envelope = tokio::select! {
            biased;

            _ = shutdown.cancelled() => {
                tracing::info!(pool = %pool_name, "shutdown requested, draining");
                break;
            }

            Some(res) = join_set.join_next() => {
                if let Err(e) = res
                    && e.is_panic() {
                        tracing::error!(pool = %pool_name, "task panicked: {:?}", e);
                    }
                continue;
            }

            envelope = receiver.recv() => {
                match envelope {
                    Some(e) => e,
                    None => break,
                }
            }
        };

        let permit = tokio::select! {
            biased;

            _ = shutdown.cancelled() => {
                tracing::info!(pool = %pool_name, "shutdown requested while waiting for permit; failing dequeued task");
                fail_envelope_shutdown(envelope, &pool_name);
                break;
            }

            permit = semaphore.clone().acquire_owned() => {
                match permit {
                    Ok(p) => p,
                    Err(_) => {
                        fail_envelope_shutdown(envelope, &pool_name);
                        break;
                    }
                }
            }
        };

        let handler = handler.clone();
        let pool = pool_name.clone();
        join_set.spawn(async move {
            let _permit = permit;
            run_task(pool, handler, envelope).await;
        });

        // Reap completed tasks without blocking.
        while let Some(res) = join_set.try_join_next() {
            if let Err(e) = res
                && e.is_panic()
            {
                tracing::error!(pool = %pool_name, "task panicked: {:?}", e);
            }
        }
    }

    while let Some(res) = join_set.join_next().await {
        if let Err(e) = res
            && e.is_panic()
        {
            tracing::error!(pool = %pool_name, "panic during drain: {:?}", e);
        }
    }

    tracing::info!(pool = %pool_name, "pool runner exited");
}

async fn run_task<I, O>(pool_name: String, handler: Arc<dyn Handler<I, O>>, env: Envelope<I, O>)
where
    I: Send + 'static,
    O: Send + Clone + 'static,
{
    let task_id = env.id;
    let worker_id = format!("{pool_name}/{task_id}");

    let (emit_tx, mut emit_rx) = mpsc::channel::<Event<O>>(env.event_buffer);
    let bcast_tx = env.events_bcast.clone();

    tokio::spawn(async move {
        while let Some(ev) = emit_rx.recv().await {
            let _ = bcast_tx.send(ev);
        }
    });

    tracing::debug!(pool = %pool_name, task_id = %task_id, "task started");
    let result = handler.handle(env.input, emit_tx, env.cancel).await;

    match &result {
        Ok(_) => tracing::debug!(pool = %pool_name, task_id = %task_id, "task succeeded"),
        Err(e) => tracing::warn!(pool = %pool_name, task_id = %task_id, error = %e, "task failed"),
    }

    let final_event = match &result {
        Ok(v) => Event::result(task_id, &worker_id, v.clone()),
        Err(e) => Event::error(task_id, &worker_id, e.to_string()),
    };
    let _ = env.events_bcast.send(final_event);
    let _ = env.result_tx.send(result);
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc;

    use super::*;

    struct EchoHandler;

    #[async_trait::async_trait]
    impl Handler<u32, u32> for EchoHandler {
        async fn handle(
            &self,
            task: u32,
            _emit: mpsc::Sender<Event<u32>>,
            _cancel: CancellationToken,
        ) -> AppResult<u32> {
            Ok(task)
        }
    }

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

    #[tokio::test]
    async fn closed_pool_submit_reports_service_unavailable_for_each_policy() {
        for overflow_policy in [
            OverflowPolicy::Block,
            OverflowPolicy::Reject,
            OverflowPolicy::DropOldest,
        ] {
            let pool = Pool::new(
                Arc::new(EchoHandler),
                PoolConfig::new("closed")
                    .with_queue_size(1)
                    .with_overflow_policy(overflow_policy),
            );
            pool.close();

            let error = match pool.submit(1).await {
                Ok(handle) => handle.result().await.unwrap_err(),
                Err(error) => error,
            };

            assert_eq!(error.code(), ErrorCode::ServiceUnavailable);
        }
    }

    #[tokio::test]
    async fn pool_stats_and_successful_result_are_reported() {
        let pool = Pool::new(
            Arc::new(EchoHandler),
            PoolConfig::new("echo")
                .with_size(0)
                .with_grace_period(Duration::from_millis(50)),
        );

        let stats = pool.stats();
        assert_eq!(stats.name, "echo");
        assert_eq!(stats.capacity, 1);
        assert!(pool.available_permits() <= 1);

        let handle = pool.submit(7).await.unwrap();
        let mut events = handle.events();
        assert_eq!(handle.result().await.unwrap(), 7);
        let event = events.try_recv().unwrap();
        assert_eq!(event.data, Some(7));

        pool.shutdown().await.unwrap();
    }
}
