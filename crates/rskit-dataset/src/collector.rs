//! Collector — orchestrates source → transform → target pipelines.
//!
//! Uses an event-driven worker pool for parallel source fetching:
//! - **Workers** pull sources from a shared channel, fetch items, apply transforms,
//!   save to disk, and send lightweight events back via mpsc.
//! - **Main loop** receives events, owns all mutable state (result, manifest, progress),
//!   and drives the progress callback from a single context (no shared mutexes).
//!
//! Supports incremental builds via `.manifest.json` caching.

use rskit_cli::CancellationToken;
use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::manifest::{CacheStatus, Manifest, SourceStats};
use crate::source::Source;
use crate::target::{PublishResult, Target};
use crate::transform::Transform;
use crate::{DataItem, Label};

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// Shared work receiver — workers take turns pulling the next source.
type WorkReceiver =
    Arc<tokio::sync::Mutex<mpsc::Receiver<(usize, Box<dyn Source>, Option<SourceStats>)>>>;

// ── Configuration ────────────────────────────────────────────────────────

/// Configuration for the collector.
#[derive(Debug, Clone)]
pub struct CollectorConfig {
    pub output_dir: PathBuf,
    pub concurrency: usize,
    pub source_timeout_secs: f64,
    pub force: bool,
}

impl Default for CollectorConfig {
    fn default() -> Self {
        Self {
            output_dir: PathBuf::from("dataset_build"),
            concurrency: 4,
            source_timeout_secs: 600.0,
            force: false,
        }
    }
}

// ── Result ───────────────────────────────────────────────────────────────

/// Result of a collection run.
#[derive(Debug, Clone, Default)]
pub struct CollectorResult {
    pub total_items: usize,
    pub real_count: usize,
    pub ai_count: usize,
    pub source_stats: std::collections::HashMap<String, SourceStats>,
    pub cached_sources: Vec<String>,
    pub publish_results: Vec<PublishResult>,
    pub duration_seconds: f64,
    pub output_dir: PathBuf,
}

// ── Worker Events ────────────────────────────────────────────────────────

/// How a source fetch concluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceOutcome {
    Done,
    TimedOut,
    Cancelled,
}

/// Events sent from worker tasks to the main event loop.
enum WorkerEvent {
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

// ── Worker Context ───────────────────────────────────────────────────────

/// Shared immutable context cloned into each worker task.
struct WorkerContext {
    transforms: Arc<Vec<Box<dyn Transform>>>,
    file_counter: Arc<AtomicUsize>,
    real_dir: PathBuf,
    ai_dir: PathBuf,
    timeout_secs: f64,
    cancel: CancellationToken,
    event_tx: mpsc::UnboundedSender<WorkerEvent>,
}

// ── Progress Callback ────────────────────────────────────────────────────

/// Callback protocol for reporting collection progress.
///
/// Called **only from the main event loop** — never from worker tasks.
/// This means implementations do NOT need interior mutability or `Sync`.
///
/// All methods have default no-op implementations, so you only override
/// what you need.
pub trait ProgressCallback: Send {
    fn on_source_start(&self, _index: usize, _name: &str, _max_items: Option<usize>) {}
    fn on_source_progress(&self, _index: usize, _count: usize) {}
    fn on_source_done(&self, _index: usize, _name: &str, _stats: &SourceStats) {}
    fn on_source_cached(&self, _index: usize, _name: &str, _stats: &SourceStats) {}
    fn on_source_error(&self, _index: usize, _name: &str, _error: &str) {}
    fn on_publish_start(&self, _target: &str) {}
    fn on_publish_done(&self, _target: &str, _result: &PublishResult) {}
    fn on_publish_error(&self, _target: &str, _error: &str) {}
}

/// No-op progress callback (all defaults).
pub struct NullProgress;
impl ProgressCallback for NullProgress {}

// ── Collector ────────────────────────────────────────────────────────────

/// Orchestrate data collection from sources through transforms to targets.
pub struct Collector {
    sources: Vec<Box<dyn Source>>,
    targets: Vec<Box<dyn Target>>,
    transforms: Arc<Vec<Box<dyn Transform>>>,
    config: CollectorConfig,
    progress: Box<dyn ProgressCallback>,
}

impl Collector {
    pub fn new(
        sources: Vec<Box<dyn Source>>,
        transforms: Vec<Box<dyn Transform>>,
        targets: Vec<Box<dyn Target>>,
        config: CollectorConfig,
        progress: Box<dyn ProgressCallback>,
    ) -> Self {
        Self {
            sources,
            targets,
            transforms: Arc::new(transforms),
            config,
            progress,
        }
    }

    /// Execute the full collection pipeline with parallel source fetching.
    ///
    /// Internally spawns a worker pool that communicates via channels.
    /// The main loop owns all mutable state — no shared mutexes.
    pub async fn run(self, cancel: &CancellationToken) -> AppResult<CollectorResult> {
        let start = Instant::now();
        let cfg = &self.config;
        let out = &cfg.output_dir;

        // Prepare output directories
        let real_dir = out.join("real");
        let ai_dir = out.join("ai");
        std::fs::create_dir_all(&real_dir).map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to create real dir: {e}"),
            )
        })?;
        std::fs::create_dir_all(&ai_dir).map_err(|e| {
            AppError::new(ErrorCode::Internal, format!("failed to create ai dir: {e}"))
        })?;

        let mut result = CollectorResult {
            output_dir: out.clone(),
            ..Default::default()
        };

        let total_sources = self.sources.len();

        // Load manifest for cache checking
        let mut manifest = if cfg.force {
            Manifest::default()
        } else {
            Manifest::load(out)
        };

        // Separate cached vs to-fetch (partial sources are resumed, not skipped)
        let mut sources_to_fetch: Vec<(usize, Box<dyn Source>, Option<SourceStats>)> = Vec::new();

        for (idx, mut source) in self.sources.into_iter().enumerate() {
            if !cfg.force {
                let cache_key = source.cache_key();
                match manifest.cache_status(source.name(), &cache_key, source.max_items()) {
                    CacheStatus::Done(stats) => {
                        result.total_items += stats.total;
                        result.real_count += stats.real;
                        result.ai_count += stats.ai;
                        result
                            .source_stats
                            .insert(source.name().to_string(), stats.clone());
                        result.cached_sources.push(source.name().to_string());
                        self.progress.on_source_cached(idx, source.name(), &stats);
                        tracing::debug!(
                            source = source.name(),
                            total = stats.total,
                            "cached, skipping"
                        );
                        continue;
                    }
                    CacheStatus::Partial(stats) => {
                        tracing::debug!(
                            source = source.name(),
                            total = stats.total,
                            offset = stats.fetched_offset,
                            "partial cache, resuming"
                        );
                        source.set_resume_state(stats.fetched_offset, stats.total);
                        sources_to_fetch.push((idx, source, Some(stats)));
                    }
                    CacheStatus::NotCached => {
                        sources_to_fetch.push((idx, source, None));
                    }
                }
            } else {
                sources_to_fetch.push((idx, source, None));
            }
        }

        if !sources_to_fetch.is_empty() {
            // File counter past existing files
            let existing = count_files(&real_dir) + count_files(&ai_dir);
            let file_counter = Arc::new(AtomicUsize::new(existing));

            // Channels: work distribution + event reporting
            let (work_tx, work_rx) =
                mpsc::channel::<(usize, Box<dyn Source>, Option<SourceStats>)>(total_sources);
            let work_rx = Arc::new(tokio::sync::Mutex::new(work_rx));
            let (event_tx, mut event_rx) = mpsc::unbounded_channel::<WorkerEvent>();

            // Spawn worker pool
            let num_workers = cfg.concurrency.min(sources_to_fetch.len());
            let mut worker_handles = Vec::with_capacity(num_workers);

            for _ in 0..num_workers {
                let ctx = WorkerContext {
                    transforms: self.transforms.clone(),
                    file_counter: file_counter.clone(),
                    real_dir: real_dir.clone(),
                    ai_dir: ai_dir.clone(),
                    timeout_secs: cfg.source_timeout_secs,
                    cancel: cancel.clone(),
                    event_tx: event_tx.clone(),
                };
                let rx = work_rx.clone();

                worker_handles.push(tokio::spawn(worker_loop(rx, ctx)));
            }

            // Drop our copy so the event channel closes when all workers finish
            drop(event_tx);

            // Send all sources into the work channel
            for (idx, source, resume_stats) in sources_to_fetch {
                // If the channel is full or closed, just break
                if work_tx.send((idx, source, resume_stats)).await.is_err() {
                    break;
                }
            }
            // Close the work channel — workers exit when drained
            drop(work_tx);

            // ── Main event loop ──────────────────────────────────────
            let mut completed_count = 0usize;
            loop {
                tokio::select! {
                    event = event_rx.recv() => {
                        match event {
                            Some(WorkerEvent::Started { index, ref name, max_items }) => {
                                self.progress.on_source_start(index, name, max_items);
                            }
                            Some(WorkerEvent::Progress { index, count }) => {
                                self.progress.on_source_progress(index, count);
                            }
                            Some(WorkerEvent::Completed { index, ref name, ref stats, ref cache_key, outcome }) => {
                                result.total_items += stats.total;
                                result.real_count += stats.real;
                                result.ai_count += stats.ai;
                                result.source_stats.insert(name.clone(), stats.clone());

                                match outcome {
                                    SourceOutcome::Done => {
                                        manifest.mark_done(name.clone(), cache_key.clone(), stats.clone());
                                    }
                                    SourceOutcome::TimedOut | SourceOutcome::Cancelled => {
                                        manifest.mark_partial(name.clone(), cache_key.clone(), stats.clone());
                                    }
                                }
                                if let Err(e) = manifest.save(out) {
                                    tracing::warn!(error = %e, "failed to save manifest");
                                }

                                self.progress.on_source_done(index, name, stats);
                                completed_count += 1;
                            }
                            Some(WorkerEvent::Failed { index, ref name, ref error, ref stats, ref cache_key }) => {
                                result.source_stats.insert(name.clone(), stats.clone());
                                // Cache partial results so next run skips re-downloading
                                if stats.total > 0 {
                                    result.total_items += stats.total;
                                    result.real_count += stats.real;
                                    result.ai_count += stats.ai;
                                    manifest.mark_partial(name.clone(), cache_key.clone(), stats.clone());
                                    if let Err(e) = manifest.save(out) {
                                        tracing::warn!(error = %e, "failed to save manifest");
                                    }
                                }
                                self.progress.on_source_error(index, name, error);
                                completed_count += 1;
                            }
                            None => break, // All workers done, channel closed
                        }
                    }
                    _ = cancel.cancelled() => {
                        tracing::debug!(completed = completed_count, "cancelled, waiting for workers");
                        break;
                    }
                }
            }

            // Wait for all worker tasks to finish
            for handle in worker_handles {
                let _ = handle.await;
            }

            // Drain any remaining events after cancellation
            while let Ok(event) = event_rx.try_recv() {
                match event {
                    WorkerEvent::Completed {
                        ref name,
                        ref stats,
                        ref cache_key,
                        outcome,
                        index,
                    } => {
                        result.total_items += stats.total;
                        result.real_count += stats.real;
                        result.ai_count += stats.ai;
                        result.source_stats.insert(name.clone(), stats.clone());
                        match outcome {
                            SourceOutcome::Done => {
                                manifest.mark_done(name.clone(), cache_key.clone(), stats.clone())
                            }
                            _ => manifest.mark_partial(
                                name.clone(),
                                cache_key.clone(),
                                stats.clone(),
                            ),
                        }
                        self.progress.on_source_done(index, name, stats);
                    }
                    WorkerEvent::Failed {
                        ref name,
                        ref error,
                        ref stats,
                        index,
                        ref cache_key,
                    } => {
                        result.source_stats.insert(name.clone(), stats.clone());
                        if stats.total > 0 {
                            result.total_items += stats.total;
                            result.real_count += stats.real;
                            result.ai_count += stats.ai;
                            manifest.mark_partial(name.clone(), cache_key.clone(), stats.clone());
                        }
                        self.progress.on_source_error(index, name, error);
                    }
                    _ => {}
                }
            }
        }

        // ── Publish to targets ───────────────────────────────────────
        for target in &self.targets {
            if cancel.is_cancelled() {
                break;
            }
            self.progress.on_publish_start(target.name());
            tracing::debug!(target = target.name(), "publishing");
            match target.publish(out, None).await {
                Ok(pub_result) => {
                    self.progress.on_publish_done(target.name(), &pub_result);
                    result.publish_results.push(pub_result);
                }
                Err(e) => {
                    self.progress
                        .on_publish_error(target.name(), &e.to_string());
                    tracing::error!(target = target.name(), error = %e, "publish failed");
                }
            }
        }

        result.duration_seconds = start.elapsed().as_secs_f64();

        // Save final manifest
        manifest.save(out)?;

        tracing::debug!(
            total = result.total_items,
            real = result.real_count,
            ai = result.ai_count,
            duration = format!("{:.1}s", result.duration_seconds),
            "collection complete"
        );

        Ok(result)
    }
}

// ── Worker implementation ────────────────────────────────────────────────

/// Long-lived worker task — pulls sources from the shared work channel
/// and processes them sequentially until the channel is drained.
async fn worker_loop(work_rx: WorkReceiver, ctx: WorkerContext) {
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

/// Process a single source: fetch items, apply transforms, save to disk,
/// and report events back to the main loop.
async fn process_source(
    idx: usize,
    source: Box<dyn Source>,
    ctx: &WorkerContext,
    resume_stats: Option<SourceStats>,
) {
    let name = source.name().to_string();
    let max_items = source.max_items();

    let resume_total = resume_stats.as_ref().map(|s| s.total).unwrap_or(0);
    let resume_real = resume_stats.as_ref().map(|s| s.real).unwrap_or(0);
    let resume_ai = resume_stats.as_ref().map(|s| s.ai).unwrap_or(0);

    // Notify main loop that we're starting
    let _ = ctx.event_tx.send(WorkerEvent::Started {
        index: idx,
        name: name.clone(),
        max_items,
    });

    // Send initial progress for resumed sources so the bar shows the starting point
    if resume_total > 0 {
        let _ = ctx.event_tx.send(WorkerEvent::Progress {
            index: idx,
            count: resume_total,
        });
    }

    tracing::debug!(source = name.as_str(), resume = resume_total, "fetching");

    // Per-source counters — start from resume values so progress is cumulative
    let src_total = AtomicUsize::new(resume_total);
    let src_real = AtomicUsize::new(resume_real);
    let src_ai = AtomicUsize::new(resume_ai);

    let source_start = Instant::now();
    let timeout_secs = ctx.timeout_secs;
    let event_tx = ctx.event_tx.clone();

    // The callback captures shared refs to atomics (Send-safe) and a cloned sender
    let mut callback = |item: DataItem| -> bool {
        // Soft timeout check
        if timeout_secs > 0.0 && source_start.elapsed().as_secs_f64() > timeout_secs {
            return false;
        }

        // Apply transforms
        let mut transformed: Option<DataItem> = Some(item);
        for t in ctx.transforms.iter() {
            transformed = transformed.and_then(|i| t.apply(i));
        }
        let transformed = match transformed {
            Some(item) => item,
            None => return true, // Filtered out, continue
        };

        // Save to disk
        let file_idx = ctx.file_counter.fetch_add(1, Ordering::SeqCst);
        let subdir = if transformed.label == Label::Real {
            &ctx.real_dir
        } else {
            &ctx.ai_dir
        };
        let path = subdir.join(format!("{:06}{}", file_idx, transformed.extension));

        if let Err(e) = std::fs::write(&path, &transformed.content) {
            tracing::error!(path = %path.display(), error = %e, "failed to write file");
            return true;
        }

        // Update counters
        if transformed.label == Label::Real {
            src_real.fetch_add(1, Ordering::Relaxed);
        } else {
            src_ai.fetch_add(1, Ordering::Relaxed);
        }
        let total = src_total.fetch_add(1, Ordering::Relaxed) + 1;

        // Send lightweight progress event
        let _ = event_tx.send(WorkerEvent::Progress {
            index: idx,
            count: total,
        });

        true
    };

    // Fetch with hard timeout via select!
    let mut timed_out = false;
    let fetch_result = if timeout_secs > 0.0 {
        tokio::select! {
            result = source.fetch(&ctx.cancel, &mut callback) => result,
            _ = ctx.cancel.cancelled() => Ok(0),
            _ = tokio::time::sleep(Duration::from_secs_f64(timeout_secs)) => {
                timed_out = true;
                Ok(0)
            }
        }
    } else {
        tokio::select! {
            result = source.fetch(&ctx.cancel, &mut callback) => result,
            _ = ctx.cancel.cancelled() => Ok(0),
        }
    };

    // Release callback borrows on atomics
    let _ = callback;

    let stats = SourceStats {
        total: src_total.load(Ordering::Relaxed),
        real: src_real.load(Ordering::Relaxed),
        ai: src_ai.load(Ordering::Relaxed),
        fetched_offset: source.last_offset(),
    };

    // Report outcome to main loop
    match fetch_result {
        Ok(_) => {
            let outcome = if ctx.cancel.is_cancelled() {
                SourceOutcome::Cancelled
            } else if timed_out {
                SourceOutcome::TimedOut
            } else {
                SourceOutcome::Done
            };
            let _ = ctx.event_tx.send(WorkerEvent::Completed {
                index: idx,
                name,
                stats,
                cache_key: source.cache_key(),
                outcome,
            });
        }
        Err(e) => {
            let err_str = e.to_string();
            let short_err = if err_str.len() > 120 {
                format!("{}…", &err_str[..120])
            } else {
                err_str
            };
            let _ = ctx.event_tx.send(WorkerEvent::Failed {
                index: idx,
                name,
                error: short_err,
                stats,
                cache_key: source.cache_key(),
            });
        }
    }
}

fn count_files(dir: &Path) -> usize {
    if !dir.exists() {
        return 0;
    }
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_file())
                .count()
        })
        .unwrap_or(0)
}
