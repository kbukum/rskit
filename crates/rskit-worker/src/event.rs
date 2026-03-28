use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Kind of event emitted by a worker during task execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventKind {
    Progress,
    Partial,
    Log,
    Result,
    Error,
}

/// Progress information for a long-running task.
#[derive(Debug, Clone)]
pub struct Progress {
    pub current: u64,
    pub total: Option<u64>,
    pub percent: Option<f32>,
    pub message: Option<String>,
}

impl Progress {
    pub fn new(current: u64, total: Option<u64>) -> Self {
        let percent = total.map(|t| {
            if t == 0 {
                100.0
            } else {
                (current as f32 / t as f32) * 100.0
            }
        });
        Self { current, total, percent, message: None }
    }

    pub fn with_message(mut self, msg: impl Into<String>) -> Self {
        self.message = Some(msg.into());
        self
    }
}

/// Event emitted by a worker task on the event channel.
#[derive(Debug, Clone)]
pub struct Event<O: Clone> {
    pub kind: EventKind,
    pub task_id: Uuid,
    pub worker_id: String,
    pub progress: Option<Progress>,
    pub data: Option<O>,
    pub error: Option<String>,
    pub timestamp: DateTime<Utc>,
}

impl<O: Clone> Event<O> {
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
