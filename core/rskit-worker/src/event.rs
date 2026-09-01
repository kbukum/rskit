use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Kind of event emitted by a worker during task execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Progress {
    /// Number of units completed so far.
    pub current: u64,
    /// Total number of units, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    /// Completion percentage on a 0–100 scale, derived from `current / total`, if `total` is known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percent: Option<f32>,
    /// Optional human-readable status message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event<O: Clone> {
    /// The kind of this event.
    #[serde(rename = "type")]
    pub kind: EventKind,
    /// Identifier of the task that produced this event, serialized as a string UUID.
    pub task_id: Uuid,
    /// Identifier of the worker that produced this event.
    pub worker_id: String,
    /// Progress snapshot, present for `EventKind::Progress` events.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<Progress>,
    /// Task output payload, present for `Partial` and `Result` events.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<O>,
    /// Error or log message, present for `Error` and `Log` events.
    #[serde(default, skip_serializing_if = "Option::is_none")]
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed_event(progress: Progress) -> Event<serde_json::Value> {
        Event {
            kind: EventKind::Progress,
            task_id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
            worker_id: "w1".to_string(),
            progress: Some(progress),
            data: None,
            error: None,
            timestamp: DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        }
    }

    #[test]
    fn progress_uses_0_to_100_percent_scale() {
        assert_eq!(Progress::new(5, Some(10)).percent, Some(50.0));
        assert_eq!(Progress::new(0, Some(0)).percent, Some(100.0));
        assert_eq!(Progress::new(3, None).percent, None);
    }

    #[test]
    fn event_kind_serializes_as_snake_case_type_field() {
        assert_eq!(
            serde_json::to_value(EventKind::Progress).unwrap(),
            serde_json::json!("progress")
        );
        assert_eq!(
            serde_json::to_value(EventKind::Result).unwrap(),
            serde_json::json!("result")
        );
        let event = fixed_event(Progress::new(1, Some(2)));
        assert_eq!(
            serde_json::to_value(&event).unwrap()["type"],
            serde_json::json!("progress")
        );
    }

    #[test]
    fn progress_event_matches_cross_kit_golden_json() {
        let event = fixed_event(Progress::new(5, Some(10)));
        let actual = serde_json::to_string_pretty(&event).unwrap();
        let expected = include_str!("../tests/fixtures/cross-kit/worker/progress-event.json");
        assert_eq!(format!("{actual}\n"), expected);

        let decoded: Event<serde_json::Value> = serde_json::from_str(expected).unwrap();
        assert_eq!(decoded.kind, EventKind::Progress);
        assert_eq!(decoded.task_id, event.task_id);
        assert_eq!(decoded.progress.unwrap().percent, Some(50.0));
    }

    #[test]
    fn unknown_total_omits_total_and_percent() {
        let event = fixed_event(Progress::new(5, None));
        let actual = serde_json::to_string_pretty(&event).unwrap();
        let expected =
            include_str!("../tests/fixtures/cross-kit/worker/progress-event-unknown-total.json");
        assert_eq!(format!("{actual}\n"), expected);

        let decoded: Event<serde_json::Value> = serde_json::from_str(expected).unwrap();
        let progress = decoded.progress.unwrap();
        assert_eq!(progress.current, 5);
        assert!(progress.total.is_none());
        assert!(progress.percent.is_none());
    }

    #[test]
    fn task_id_serializes_as_string_uuid() {
        let event = fixed_event(Progress::new(1, Some(2)));
        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(
            value["task_id"],
            serde_json::json!("00000000-0000-0000-0000-000000000001")
        );
    }
}
