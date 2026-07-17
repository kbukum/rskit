//! Collector engine — orchestrates the worker pool and the single-owner main event loop.
//!
//! The engine is generic over any [`DatasetItem`] type and item-agnostic:
//! - **Workers** pull sources from a shared channel, fetch items, apply transforms, validate, and
//!   hand each item to the injected [`ItemSink`], sending lightweight events back via mpsc.
//! - **Main loop** receives events, owns all mutable state (result, manifest, progress), and drives
//!   the progress callback from a single context (no shared mutexes).
//!
//! Supports incremental builds via `.manifest.json` caching.

use std::sync::Arc;
use std::time::Instant;

use rskit_errors::AppResult;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::DatasetItem;
use crate::manifest::{CacheStatus, Manifest};
use crate::sink::ItemSink;
use crate::source::Source;
use crate::target::Target;
use crate::transform::Transform;
use crate::validate::Validator;

use super::config::CollectorConfig;
use super::event::{WorkerEvent, handle_drained_worker_event, handle_worker_event};
use super::progress::ProgressCallback;
use super::result::CollectorResult;
use super::worker::{WorkItem, WorkerContext, load_manifest, save_manifest, worker_loop};

/// Orchestrate data collection from sources through transforms and validation to a sink and targets.
pub struct Collector<T: DatasetItem> {
    sources: Vec<Box<dyn Source<T>>>,
    targets: Vec<Box<dyn Target>>,
    transforms: Arc<Vec<Box<dyn Transform<T, T>>>>,
    sink: Arc<dyn ItemSink<T>>,
    validator: Option<Arc<dyn Validator<T>>>,
    config: CollectorConfig,
    progress: Box<dyn ProgressCallback>,
}

impl<T: DatasetItem> Collector<T> {
    /// Create a collector from explicit source, transform, sink, target, config, and progress
    /// contracts. Validation is off by default; add it with [`with_validator`](Self::with_validator).
    pub fn new(
        sources: Vec<Box<dyn Source<T>>>,
        transforms: Vec<Box<dyn Transform<T, T>>>,
        sink: Arc<dyn ItemSink<T>>,
        targets: Vec<Box<dyn Target>>,
        config: CollectorConfig,
        progress: Box<dyn ProgressCallback>,
    ) -> Self {
        Self {
            sources,
            targets,
            transforms: Arc::new(transforms),
            sink,
            validator: None,
            config,
            progress,
        }
    }

    /// Reject items that fail the given validator before they reach the sink.
    #[must_use]
    pub fn with_validator(mut self, validator: Arc<dyn Validator<T>>) -> Self {
        self.validator = Some(validator);
        self
    }

    /// Execute the full collection pipeline with parallel source fetching.
    ///
    /// Internally spawns a worker pool that communicates via channels.
    /// The main loop owns all mutable state — no shared mutexes.
    pub async fn run(self, cancel: &CancellationToken) -> AppResult<CollectorResult> {
        let start = Instant::now();
        let cfg = &self.config;
        let out = &cfg.output_dir;

        self.sink.prepare().await?;

        let mut result = CollectorResult {
            output_dir: out.clone(),
            ..Default::default()
        };

        let total_sources = self.sources.len();

        // Load manifest for cache checking
        let mut manifest = if cfg.force {
            Manifest::default()
        } else {
            load_manifest(out.to_path_buf()).await?
        };

        // Separate cached vs to-fetch (partial sources are resumed, not skipped)
        let mut sources_to_fetch: Vec<WorkItem<T>> = Vec::new();

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
            // Channels: work distribution + event reporting
            let (work_tx, work_rx) = mpsc::channel::<WorkItem<T>>(total_sources);
            let work_rx = Arc::new(tokio::sync::Mutex::new(work_rx));
            let event_capacity = cfg
                .limits
                .stream_buffer
                .max(total_sources.saturating_mul(4))
                .max(1);
            let (event_tx, mut event_rx) = mpsc::channel::<WorkerEvent>(event_capacity);

            // Spawn worker pool
            let num_workers = cfg.concurrency.max(1).min(sources_to_fetch.len());
            let mut worker_handles = Vec::with_capacity(num_workers);

            for _ in 0..num_workers {
                let ctx = WorkerContext {
                    transforms: self.transforms.clone(),
                    sink: self.sink.clone(),
                    validator: self.validator.clone(),
                    timeout_secs: cfg.source_timeout_secs,
                    limits: cfg.limits,
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
            let mut cancellation_seen = false;
            loop {
                tokio::select! {
                    event = event_rx.recv() => {
                        match event {
                            Some(event) => {
                                let (completed, should_save) = handle_worker_event(
                                    event,
                                    &mut result,
                                    &mut manifest,
                                    self.progress.as_ref(),
                                );
                                if should_save
                                    && let Err(e) =
                                        save_manifest(out.to_path_buf(), manifest.clone()).await
                                {
                                    tracing::warn!(error = %e, "failed to save manifest");
                                }
                                if completed {
                                    completed_count += 1;
                                }
                            }
                            None => break, // All workers done, channel closed
                        }
                    }
                    _ = cancel.cancelled(), if !cancellation_seen => {
                        tracing::debug!(completed = completed_count, "cancelled, waiting for workers");
                        cancellation_seen = true;
                    }
                }
            }

            // Wait for all worker tasks to finish
            for handle in worker_handles {
                let _ = handle.await;
            }

            // Drain any remaining events after cancellation
            while let Ok(event) = event_rx.try_recv() {
                handle_drained_worker_event(
                    event,
                    &mut result,
                    &mut manifest,
                    self.progress.as_ref(),
                );
            }
        }

        // Flush the sink before publishing so targets observe a complete dataset.
        self.sink.finish().await?;

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
        save_manifest(out.to_path_buf(), manifest).await?;

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
