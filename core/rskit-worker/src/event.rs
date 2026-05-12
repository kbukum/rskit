use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Kind of event emitted by a worker during task execution.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EventKind {
    /// Periodic progress update with completion percentage.
    Progress,
    /// A partial result emitted before the task finishes.
    Partial,
    /// A free-form log message from the task.
    Log,
    /// The final successful result of the task.
    Result,
    /// The task failed with this error message.
    Error,
}

/// Progress information for a long-running task.
#[derive(Debug, Clone)]
pub struct Progress {
    /// Number of units completed so far.
    pub current: u64,
    /// Total number of units, if known.
    pub total: Option<u64>,
    /// Completion percentage derived from `current / total`, if `total` is known.
    pub percent: Option<f32>,
    /// Optional human-readable status message.
    pub message: Option<String>,
}

impl Progress {
    /// Create a new `Progress` value, computing `percent` when `total` is provided.
    pub fn new(current: u64, total: Option<u64>) -> Self {
        let percent = total.map(|t| {
            if t == 0 {
                100.0
            } else {
                (current as f32 / t as f32) * 100.0
            }
        });
        Self {
            current,
            total,
            percent,
            message: None,
        }
    }

    /// Attach a human-readable status message to this progress value.
    #[must_use]
    pub fn with_message(mut self, msg: impl Into<String>) -> Self {
        self.message = Some(msg.into());
        self
    }
}

/// Event emitted by a worker task on the event channel.
#[derive(Debug, Clone)]
pub struct Event<O: Clone> {
    /// The kind of this event.
    pub kind: EventKind,
    /// Identifier of the task that produced this event.
    pub task_id: Uuid,
    /// Identifier of the worker that produced this event.
    pub worker_id: String,
    /// Progress snapshot, present for `EventKind::Progress` events.
    pub progress: Option<Progress>,
    /// Task output payload, present for `Partial` and `Result` events.
    pub data: Option<O>,
    /// Error or log message, present for `Error` and `Log` events.
    pub error: Option<String>,
    /// Wall-clock time at which the event was created.
    pub timestamp: DateTime<Utc>,
}

impl<O: Clone> Event<O> {
    /// Create a progress event carrying the given [`Progress`] snapshot.
    pub fn progress(task_id: Uuid, worker_id: impl Into<String>, p: Progress) -> Self {
        Self {
            kind: EventKind::Progress,
            task_id,
            worker_id: worker_id.into(),
            progress: Some(p),
            data: None,
            error: None,
            timestamp: Utc::now(),
        }
    }

    /// Create a partial-result event carrying an intermediate output value.
    pub fn partial(task_id: Uuid, worker_id: impl Into<String>, data: O) -> Self {
        Self {
            kind: EventKind::Partial,
            task_id,
            worker_id: worker_id.into(),
            progress: None,
            data: Some(data),
            error: None,
            timestamp: Utc::now(),
        }
    }

    /// Create a log event carrying a free-form text message.
    pub fn log(task_id: Uuid, worker_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: EventKind::Log,
            task_id,
            worker_id: worker_id.into(),
            progress: None,
            data: None,
            error: Some(message.into()),
            timestamp: Utc::now(),
        }
    }

    /// Create a final-result event carrying the successful task output.
    pub fn result(task_id: Uuid, worker_id: impl Into<String>, data: O) -> Self {
        Self {
            kind: EventKind::Result,
            task_id,
            worker_id: worker_id.into(),
            progress: None,
            data: Some(data),
            error: None,
            timestamp: Utc::now(),
        }
    }

    /// Create an error event carrying the failure message.
    pub fn error(task_id: Uuid, worker_id: impl Into<String>, err: impl Into<String>) -> Self {
        Self {
            kind: EventKind::Error,
            task_id,
            worker_id: worker_id.into(),
            progress: None,
            data: None,
            error: Some(err.into()),
            timestamp: Utc::now(),
        }
    }
}
