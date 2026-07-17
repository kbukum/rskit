//! Worker events and the main-loop handlers that fold them into result, manifest, and progress.

use crate::manifest::{Manifest, SourceStats};

use super::progress::ProgressCallback;
use super::result::CollectorResult;

/// How a source stream concluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceOutcome {
    /// Source reached the end of its stream.
    Done,
    /// Source exceeded the configured timeout.
    TimedOut,
    /// Source stopped because cancellation was requested.
    Cancelled,
}

/// Events sent from worker tasks to the main event loop.
pub(crate) enum WorkerEvent {
    Started {
        index: usize,
        name: String,
        max_items: Option<usize>,
    },
    Progress {
        index: usize,
        count: usize,
    },
    Completed {
        index: usize,
        name: String,
        stats: SourceStats,
        cache_key: serde_json::Value,
        outcome: SourceOutcome,
    },
    Failed {
        index: usize,
        name: String,
        error: String,
        stats: SourceStats,
        cache_key: serde_json::Value,
    },
}

pub(crate) fn handle_worker_event(
    event: WorkerEvent,
    result: &mut CollectorResult,
    manifest: &mut Manifest,
    progress: &dyn ProgressCallback,
) -> (bool, bool) {
    match event {
        WorkerEvent::Started {
            index,
            ref name,
            max_items,
        } => {
            progress.on_source_start(index, name, max_items);
            (false, false)
        }
        WorkerEvent::Progress { index, count } => {
            progress.on_source_progress(index, count);
            (false, false)
        }
        WorkerEvent::Completed {
            index,
            ref name,
            ref stats,
            ref cache_key,
            outcome,
        } => {
            record_completed_event(
                result,
                manifest,
                CompletedEventRef {
                    index,
                    name,
                    stats,
                    cache_key,
                    outcome,
                },
                progress,
            );
            (true, true)
        }
        WorkerEvent::Failed {
            index,
            ref name,
            ref error,
            ref stats,
            ref cache_key,
        } => {
            let saved = record_failed_event(
                result,
                manifest,
                FailedEventRef {
                    index,
                    name,
                    error,
                    stats,
                    cache_key,
                },
                progress,
            );
            (true, saved)
        }
    }
}

pub(crate) fn handle_drained_worker_event(
    event: WorkerEvent,
    result: &mut CollectorResult,
    manifest: &mut Manifest,
    progress: &dyn ProgressCallback,
) {
    match event {
        WorkerEvent::Completed {
            index,
            ref name,
            ref stats,
            ref cache_key,
            outcome,
        } => record_completed_event(
            result,
            manifest,
            CompletedEventRef {
                index,
                name,
                stats,
                cache_key,
                outcome,
            },
            progress,
        ),
        WorkerEvent::Failed {
            index,
            ref name,
            ref error,
            ref stats,
            ref cache_key,
        } => {
            record_failed_event(
                result,
                manifest,
                FailedEventRef {
                    index,
                    name,
                    error,
                    stats,
                    cache_key,
                },
                progress,
            );
        }
        WorkerEvent::Started { .. } | WorkerEvent::Progress { .. } => {}
    }
}

struct CompletedEventRef<'a> {
    index: usize,
    name: &'a str,
    stats: &'a SourceStats,
    cache_key: &'a serde_json::Value,
    outcome: SourceOutcome,
}

struct FailedEventRef<'a> {
    index: usize,
    name: &'a str,
    error: &'a str,
    stats: &'a SourceStats,
    cache_key: &'a serde_json::Value,
}

fn record_completed_event(
    result: &mut CollectorResult,
    manifest: &mut Manifest,
    event: CompletedEventRef<'_>,
    progress: &dyn ProgressCallback,
) {
    let CompletedEventRef {
        index,
        name,
        stats,
        cache_key,
        outcome,
    } = event;
    result.total_items += stats.total;
    result.real_count += stats.real;
    result.ai_count += stats.ai;
    result.source_stats.insert(name.to_string(), stats.clone());
    match outcome {
        SourceOutcome::Done => {
            manifest.mark_done(name.to_string(), cache_key.clone(), stats.clone())
        }
        SourceOutcome::TimedOut | SourceOutcome::Cancelled => {
            manifest.mark_partial(name.to_string(), cache_key.clone(), stats.clone());
        }
    }
    progress.on_source_done(index, name, stats);
}

fn record_failed_event(
    result: &mut CollectorResult,
    manifest: &mut Manifest,
    event: FailedEventRef<'_>,
    progress: &dyn ProgressCallback,
) -> bool {
    let FailedEventRef {
        index,
        name,
        error,
        stats,
        cache_key,
    } = event;
    result.source_stats.insert(name.to_string(), stats.clone());
    let saved = stats.total > 0;
    if saved {
        result.total_items += stats.total;
        result.real_count += stats.real;
        result.ai_count += stats.ai;
        manifest.mark_partial(name.to_string(), cache_key.clone(), stats.clone());
    }
    progress.on_source_error(index, name, error);
    saved
}
