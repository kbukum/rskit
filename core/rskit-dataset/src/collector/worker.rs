//! Worker pool — pull sources, fetch and transform items, hand them to the sink, and report events.

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::StreamExt as _;
use rskit_errors::{AppError, AppResult};
use rskit_util::strings::truncate_owned;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::manifest::{Manifest, SourceStats};
use crate::sink::ItemSink;
use crate::source::Source;
use crate::transform::Transform;
use crate::validate::Validator;
use crate::{DatasetItem, DatasetLimits, Label};

use super::event::{SourceOutcome, WorkerEvent};

/// A queued unit of work: the source's original index, the source, and any resume stats.
pub(crate) type WorkItem<T> = (usize, Box<dyn Source<T>>, Option<SourceStats>);

/// Shared work receiver — workers take turns pulling the next source.
pub(crate) type WorkReceiver<T> = Arc<tokio::sync::Mutex<mpsc::Receiver<WorkItem<T>>>>;

/// Shared immutable context cloned into each worker task.
pub(crate) struct WorkerContext<T: DatasetItem> {
    pub(crate) transforms: Arc<Vec<Box<dyn Transform<T, T>>>>,
    pub(crate) sink: Arc<dyn ItemSink<T>>,
    pub(crate) validator: Option<Arc<dyn Validator<T>>>,
    pub(crate) timeout_secs: f64,
    pub(crate) limits: DatasetLimits,
    pub(crate) cancel: CancellationToken,
    pub(crate) event_tx: mpsc::Sender<WorkerEvent>,
}

/// Long-lived worker task — pulls sources from the shared work channel
/// and processes them sequentially until the channel is drained.
pub(crate) async fn worker_loop<T: DatasetItem>(work_rx: WorkReceiver<T>, ctx: WorkerContext<T>) {
    loop {
        // Lock the receiver just long enough to pull the next source
        let task = {
            let mut rx = work_rx.lock().await;
            rx.recv().await
        };
        match task {
            Some((idx, source, resume_stats)) => {
                process_source(idx, source, &ctx, resume_stats).await
            }
            None => break, // Channel closed, no more work
        }
    }
}

/// Process a single source: fetch items, apply transforms, validate, materialize via the sink,
/// and report events back to the main loop.
async fn process_source<T: DatasetItem>(
    idx: usize,
    source: Box<dyn Source<T>>,
    ctx: &WorkerContext<T>,
    resume_stats: Option<SourceStats>,
) {
    let name = source.name().to_string();
    let max_items = source.max_items();

    let resume_total = resume_stats.as_ref().map(|s| s.total).unwrap_or(0);
    let resume_real = resume_stats.as_ref().map(|s| s.real).unwrap_or(0);
    let resume_ai = resume_stats.as_ref().map(|s| s.ai).unwrap_or(0);

    // Notify main loop that we're starting
    send_worker_event(
        ctx,
        WorkerEvent::Started {
            index: idx,
            name: name.clone(),
            max_items,
        },
    )
    .await;

    // Send initial progress for resumed sources so the bar shows the starting point
    if resume_total > 0 {
        let _ = ctx.event_tx.try_send(WorkerEvent::Progress {
            index: idx,
            count: resume_total,
        });
    }

    tracing::debug!(source = name.as_str(), resume = resume_total, "fetching");

    let source_start = Instant::now();
    let timeout_secs = ctx.timeout_secs;
    let cache_key = source.cache_key();

    // Per-source counters — start from resume values so progress is cumulative.
    let mut total = resume_total;
    let mut real = resume_real;
    let mut ai = resume_ai;
    let mut fetched_offset = resume_stats.as_ref().map(|s| s.fetched_offset).unwrap_or(0);

    let mut stream = source.stream(ctx.cancel.clone());

    let process_stream = async {
        while let Some(item) = stream.next().await {
            if timeout_secs > 0.0 && source_start.elapsed().as_secs_f64() > timeout_secs {
                return Ok::<SourceOutcome, AppError>(SourceOutcome::TimedOut);
            }
            let item = item?;
            let pending_offset = item
                .source_offset()
                .unwrap_or_else(|| fetched_offset.saturating_add(1));
            let mut transformed = Some(item);
            for transform in ctx.transforms.iter() {
                transformed = match transformed {
                    Some(item) => transform.apply(item, &ctx.limits)?,
                    None => None,
                };
            }
            let Some(transformed) = transformed else {
                fetched_offset = pending_offset;
                continue;
            };

            if let Some(validator) = &ctx.validator {
                validator.validate(&transformed)?;
            }

            // Read routing metadata before the item is moved into the sink.
            let label = transformed.label();
            ctx.sink.write(transformed).await?;
            fetched_offset = pending_offset;

            if label == Label::Real {
                real += 1;
            } else {
                ai += 1;
            }
            total += 1;

            if !ctx.cancel.is_cancelled() {
                let _ = ctx.event_tx.try_send(WorkerEvent::Progress {
                    index: idx,
                    count: total,
                });
            }
        }
        Ok::<SourceOutcome, AppError>(SourceOutcome::Done)
    };

    // Consume with hard timeout via select.
    let mut timed_out = false;
    let fetch_result = if timeout_secs > 0.0 {
        tokio::select! {
            result = process_stream => result,
            _ = ctx.cancel.cancelled() => Ok(SourceOutcome::Cancelled),
            _ = tokio::time::sleep(Duration::from_secs_f64(timeout_secs)) => {
                timed_out = true;
                Ok(SourceOutcome::TimedOut)
            }
        }
    } else {
        tokio::select! {
            result = process_stream => result,
            _ = ctx.cancel.cancelled() => Ok(SourceOutcome::Cancelled),
        }
    };

    let stats = SourceStats {
        total,
        real,
        ai,
        fetched_offset,
    };

    // Report outcome to main loop
    match fetch_result {
        Ok(stream_outcome) => {
            let outcome = if ctx.cancel.is_cancelled() {
                SourceOutcome::Cancelled
            } else if timed_out {
                SourceOutcome::TimedOut
            } else {
                stream_outcome
            };
            send_worker_event(
                ctx,
                WorkerEvent::Completed {
                    index: idx,
                    name,
                    stats,
                    cache_key,
                    outcome,
                },
            )
            .await;
        }
        Err(e) => {
            let short_err = truncate_owned(&e.to_string(), 120);
            send_worker_event(
                ctx,
                WorkerEvent::Failed {
                    index: idx,
                    name,
                    error: short_err,
                    stats,
                    cache_key,
                },
            )
            .await;
        }
    }
}

async fn send_worker_event<T: DatasetItem>(ctx: &WorkerContext<T>, event: WorkerEvent) {
    let _ = ctx.event_tx.send(event).await;
}

pub(crate) async fn load_manifest(output_dir: std::path::PathBuf) -> AppResult<Manifest> {
    tokio::task::spawn_blocking(move || Manifest::load(&output_dir))
        .await
        .map_err(AppError::internal)?
}

pub(crate) async fn save_manifest(
    output_dir: std::path::PathBuf,
    manifest: Manifest,
) -> AppResult<()> {
    tokio::task::spawn_blocking(move || manifest.save(&output_dir))
        .await
        .map_err(AppError::internal)?
}
