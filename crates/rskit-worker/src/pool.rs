use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tokio::sync::{Notify, Semaphore, broadcast, mpsc, oneshot};
use tokio::task::JoinSet;
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

    /// Set the maximum number of concurrent tasks.
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

    fn push_reject(&self, item: T) -> Result<(), T> {
        let mut inner = self.state.inner.lock();
        if inner.closed || inner.items.len() >= inner.capacity {
            return Err(item);
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
    shutdown: CancellationToken,
}

impl<I, O> Pool<I, O>
where
    I: Send + 'static,
    O: Send + Clone + 'static,
{
    /// Create a new pool backed by `handler`.
    pub fn new(handler: Arc<dyn Handler<I, O>>, config: PoolConfig) -> Self {
        let semaphore = Arc::new(Semaphore::new(config.size));
        let (queue, receiver) = SubmitQueue::<Envelope<I, O>>::new(config.queue_size);
        let shutdown = CancellationToken::new();

        tokio::spawn(runner_loop(
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
            capacity: config.size,
            event_buffer: config.event_buffer,
            overflow_policy: config.overflow_policy,
            shutdown,
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
                self.queue.push_reject(envelope).map_err(|_| {
                    AppError::rate_limited()
                        .with_detail("pool", self.name.clone())
                        .with_detail("overflow_policy", "reject")
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

    /// Cancel all in-flight tasks and shut down the runner loop.
    pub async fn shutdown(self) -> AppResult<()> {
        self.shutdown.cancel();
        self.queue.close();
        Ok(())
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
        error.message.clone(),
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
        // Acquire a semaphore permit BEFORE popping from the queue.
        // This ensures items stay in the queue (and count toward capacity)
        // until a worker slot is actually available.
        let permit = tokio::select! {
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

            permit = semaphore.clone().acquire_owned() => {
                match permit {
                    Ok(p) => p,
                    Err(_) => break,
                }
            }
        };

        // Now that we have a worker slot, pop the next item from the queue.
        let envelope = tokio::select! {
            biased;

            _ = shutdown.cancelled() => {
                tracing::info!(pool = %pool_name, "shutdown requested, draining");
                break;
            }

            envelope = receiver.recv() => {
                match envelope {
                    Some(e) => e,
                    None => break,
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
