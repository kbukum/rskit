//! Dashboard — task status display using rskit-cli output formatting.

use crate::tasks::TaskOutput;
use rskit_cli::OutputTable;
use std::time::{Duration, Instant};

/// Tracks the state of a submitted task.
#[derive(Clone)]
pub struct TrackedTask {
    pub id: usize,
    pub label: String,
    pub status: TaskStatus,
    pub started: Instant,
    pub progress: Option<(u64, u64)>,
    pub message: String,
    pub result: Option<TaskOutput>,
}

#[derive(Clone, PartialEq)]
pub enum TaskStatus {
    Running,
    Done,
    Failed,
    Cancelled,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Running   => write!(f, "\x1b[33m⟳ Running\x1b[0m"),
            Self::Done      => write!(f, "\x1b[32m✓ Done\x1b[0m"),
            Self::Failed    => write!(f, "\x1b[31m✗ Failed\x1b[0m"),
            Self::Cancelled => write!(f, "\x1b[2m⊘ Cancelled\x1b[0m"),
        }
    }
}

/// Renders a table of all tracked tasks.
pub fn render_status_table(tasks: &[TrackedTask]) -> String {
    if tasks.is_empty() {
        return "  \x1b[2mNo tasks submitted yet.\x1b[0m".to_string();
    }

    let mut table = OutputTable::new(vec!["ID", "Task", "Status", "Progress", "Duration"]);
    for t in tasks {
        let elapsed = t.started.elapsed();
        let dur = format_duration(elapsed);
        let progress = match (&t.status, t.progress) {
            (TaskStatus::Done, _) => "100%".to_string(),
            (TaskStatus::Cancelled, _) => "—".to_string(),
            (_, Some((cur, total))) if total > 0 => {
                format!("{}%", (cur * 100) / total)
            }
            _ => if t.message.is_empty() { "—".to_string() } else { t.message.clone() },
        };
        table.add_row(vec![
            t.id.to_string(),
            t.label.clone(),
            t.status.to_string(),
            progress,
            dur,
        ]);
    }
    format!("{table}")
}

/// Formats the completion notification as a string.
pub fn render_completion(task: &TrackedTask) -> String {
    let dur = format_duration(task.started.elapsed());
    match &task.status {
        TaskStatus::Done => {
            let summary = task
                .result
                .as_ref()
                .map(|r| r.summary.as_str())
                .unwrap_or("completed");
            format!(
                "  \x1b[32m✓ Task #{} completed\x1b[0m — {} \x1b[2m({})\x1b[0m",
                task.id, summary, dur
            )
        }
        TaskStatus::Failed => {
            let msg = task
                .result
                .as_ref()
                .map(|r| r.summary.as_str())
                .unwrap_or("unknown error");
            format!(
                "  \x1b[31m✗ Task #{} failed\x1b[0m — {} \x1b[2m({})\x1b[0m",
                task.id, msg, dur
            )
        }
        TaskStatus::Cancelled => {
            format!(
                "  \x1b[2m⊘ Task #{} cancelled\x1b[0m \x1b[2m({})\x1b[0m",
                task.id, dur
            )
        }
        _ => String::new(),
    }
}

/// Render details of a specific task.
pub fn render_task_details(task: &TrackedTask) -> String {
    let mut out = String::new();
    out.push_str(&format!("  \x1b[1mTask #{}\x1b[0m — {}\n", task.id, task.label));
    out.push_str(&format!("  Status: {}\n", task.status));
    out.push_str(&format!("  Duration: {}\n", format_duration(task.started.elapsed())));
    if !task.message.is_empty() {
        out.push_str(&format!("  Last step: {}\n", task.message));
    }
    if let Some(ref output) = task.result {
        out.push_str(&format!("  Summary: {}\n", output.summary));
        if !output.details.is_empty() {
            let mut kv = rskit_cli::OutputKV::new();
            for (k, v) in &output.details {
                kv.add(k, v);
            }
            out.push_str(&format!("{kv}"));
        }
    }
    out
}

fn format_duration(d: Duration) -> String {
    if d.as_secs() >= 60 {
        format!(
            "{}m {:.1}s",
            d.as_secs() / 60,
            (d.as_secs() % 60) as f64 + d.subsec_millis() as f64 / 1000.0
        )
    } else if d.as_millis() >= 1000 {
        format!("{:.1}s", d.as_secs_f64())
    } else {
        format!("{}ms", d.as_millis())
    }
}
