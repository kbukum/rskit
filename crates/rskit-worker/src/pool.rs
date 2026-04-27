use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Semaphore, broadcast, mpsc, oneshot};
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
        }
    }
}

impl PoolConfig {
    /// Create a named pool configuration with sensible defaults.
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
}

fn available_parallelism() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

// ---------------------------------------------------------------------------
// Internal envelope sent from Pool::submit to the runner loop.
// ---------------------------------------------------------------------------
struct Envelope<I, O: Clone + Send + 'static> {
    id: Uuid,
    input: I,
    /// The pool writes events here; tasks relay them to their broadcast channel.
    events_bcast: broadcast::Sender<Event<O>>,
    result_tx: oneshot::Sender<AppResult<O>>,
    cancel: CancellationToken,
}

// ---------------------------------------------------------------------------
// Pool
// ---------------------------------------------------------------------------

/// A bounded async worker pool.
///
/// Internally:
/// - One `mpsc` channel accepts envelopes from callers.
/// - A [`Semaphore`] limits concurrency to `config.size`.
/// - A [`JoinSet`] tracks spawned tasks and detects panics.
/// - Each task gets an `mpsc::Sender<Event<O>>` to emit progress; a small
///   forwarder task relays those into a `broadcast::Sender` that callers
///   subscribe to via [`TaskHandle::events`].
pub struct Pool<I, O>
where
    I: Send + 'static,
    O: Send + Clone + 'static,
{
    name: String,
    tx: mpsc::Sender<Envelope<I, O>>,
    semaphore: Arc<Semaphore>,
    capacity: usize,
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
        let (tx, rx) = mpsc::channel::<Envelope<I, O>>(config.queue_size);
        let shutdown = CancellationToken::new();

        tokio::spawn(runner_loop(
            config.name.clone(),
            handler,
            rx,
            semaphore.clone(),
            shutdown.clone(),
        ));

        Pool {
            name: config.name,
            tx,
            semaphore,
            capacity: config.size,
            shutdown,
        }
    }

    /// Submit a task; returns a [`TaskHandle`] immediately.
    pub async fn submit(&self, input: I) -> AppResult<TaskHandle<O>> {
        let id = Uuid::new_v4();
        // broadcast for callers to subscribe to events
        let (bcast_tx, bcast_rx) = broadcast::channel::<Event<O>>(64);
        let (result_tx, result_rx) = oneshot::channel::<AppResult<O>>();
        let cancel = CancellationToken::new();

        let handle = TaskHandle::new(id, bcast_rx, result_rx, cancel.clone());

        self.tx
            .send(Envelope {
                id,
                input,
                events_bcast: bcast_tx,
                result_tx,
                cancel,
            })
            .await
            .map_err(|_| {
                AppError::new(
                    ErrorCode::ServiceUnavailable,
                    format!("pool '{}' is shut down", self.name),
                )
            })?;

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
        drop(self.tx);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Runner loop
// ---------------------------------------------------------------------------

async fn runner_loop<I, O>(
    pool_name: String,
    handler: Arc<dyn Handler<I, O>>,
    mut rx: mpsc::Receiver<Envelope<I, O>>,
    semaphore: Arc<Semaphore>,
    shutdown: CancellationToken,
) where
    I: Send + 'static,
    O: Send + Clone + 'static,
{
    let mut join_set: JoinSet<()> = JoinSet::new();

    loop {
        tokio::select! {
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
            }

            env = rx.recv() => {
                let Some(env) = env else { break };

                let permit = match semaphore.clone().acquire_owned().await {
                    Ok(p) => p,
                    Err(_) => break,
                };

                let h = handler.clone();
                let name = pool_name.clone();

                join_set.spawn(async move {
                    let _permit = permit;
                    run_task(name, h, env).await;
                });
            }
        }
    }

    // Drain in-flight tasks.
    while let Some(res) = join_set.join_next().await {
        if let Err(e) = res
            && e.is_panic()
        {
            tracing::error!(pool = %pool_name, "panic during drain: {:?}", e);
        }
    }

    tracing::info!(pool = %pool_name, "pool runner exited");
}

/// Run one task envelope.
///
/// Creates an `mpsc` channel pair for events, spawns a tiny forwarder task
/// that copies from `mpsc` → `broadcast`, then calls the handler.
async fn run_task<I, O>(pool_name: String, handler: Arc<dyn Handler<I, O>>, env: Envelope<I, O>)
where
    I: Send + 'static,
    O: Send + Clone + 'static,
{
    let task_id = env.id;
    let worker_id = format!("{pool_name}/{task_id}");

    // mpsc channel: handler → forwarder → broadcast
    let (emit_tx, mut emit_rx) = mpsc::channel::<Event<O>>(64);
    let bcast_tx = env.events_bcast.clone();

    // Forwarder: relay mpsc events to broadcast subscribers.
    tokio::spawn(async move {
        while let Some(ev) = emit_rx.recv().await {
            let _ = bcast_tx.send(ev);
        }
    });

    tracing::debug!(pool = %pool_name, task_id = %task_id, "task started");

    let result = handler.handle(env.input, emit_tx, env.cancel).await;

    match &result {
        Ok(_) => tracing::debug!(pool = %pool_name, task_id = %task_id, "task succeeded"),
        Err(e) => tracing::warn!(
            pool = %pool_name, task_id = %task_id, error = %e, "task failed"
        ),
    }

    // Emit a final result/error event on the broadcast channel.
    let final_event = match &result {
        Ok(v) => Event::result(task_id, &worker_id, v.clone()),
        Err(e) => Event::error(task_id, &worker_id, e.to_string()),
    };
    let _ = env.events_bcast.send(final_event);

    let _ = env.result_tx.send(result);
}
