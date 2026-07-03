//! [`FsWatcher`]: the `notify`-backed source that produces a debounced stream.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use futures::{Stream, StreamExt as _};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_stream::{CancellationToken, RskitStreamExt as _, from_channel};
use tokio::sync::mpsc;

use super::change::FsChangeBatch;

/// Default bounded capacity for the internal raw-event channel.
///
/// Backpressure is intentional: a slow consumer stalls the platform watcher
/// thread rather than growing an unbounded queue.
const DEFAULT_BUFFER: usize = 1024;

/// Safety cap on the number of raw events coalesced into one debounced batch.
///
/// The debounce timer resets on every event, so a sustained change rate faster
/// than the debounce window (e.g. a large checkout) would otherwise accumulate
/// unboundedly; reaching this many events force-flushes an early batch. It is
/// generous so ordinary edit bursts still coalesce into a single batch.
const MAX_BATCH_EVENTS: usize = 65_536;

/// A single raw watcher event bridged from the platform callback onto the
/// internal channel, before debouncing and coalescing into an [`FsChangeBatch`].
enum RawEvent {
    /// A path the watcher reported as changed.
    Changed(PathBuf),
    /// The watcher reported an error (typically a queue overflow), so some
    /// notifications may have been dropped and the tree should be rescanned.
    Rescan,
}

/// Coalesce a debounce window's worth of [`RawEvent`]s into one [`FsChangeBatch`]:
/// deduplicate changed paths and set the rescan flag if any overflow was seen.
fn batch_from_raw(events: Vec<RawEvent>) -> FsChangeBatch {
    let mut paths = BTreeSet::new();
    let mut rescan = false;
    for event in events {
        match event {
            RawEvent::Changed(path) => {
                paths.insert(path);
            }
            RawEvent::Rescan => rescan = true,
        }
    }
    FsChangeBatch::new(paths).with_rescan(rescan)
}

/// An owned, bounded stream of debounced [`FsChangeBatch`]es.
///
/// Boxed so the concrete `notify`/channel machinery stays private and callers
/// (including trait-object ports) depend only on `Stream<Item = FsChangeBatch>`.
pub type FsChangeStream = Pin<Box<dyn Stream<Item = FsChangeBatch> + Send>>;

/// A recursive, debounced filesystem-tree watcher.
///
/// Construct with a debounce window, then call [`watch`](Self::watch) with the
/// roots to observe and a [`CancellationToken`]. Each call is independent: it
/// installs its own platform watcher, kept alive for exactly as long as the
/// returned stream — dropping the stream (or firing the token) tears the OS
/// watch down.
#[derive(Debug, Clone)]
pub struct FsWatcher {
    debounce: Duration,
    buffer: usize,
}

impl FsWatcher {
    /// Create a watcher with the given trailing-edge debounce window.
    #[must_use]
    pub const fn new(debounce: Duration) -> Self {
        Self {
            debounce,
            buffer: DEFAULT_BUFFER,
        }
    }

    /// Override the bounded channel capacity (clamped to at least 1).
    #[must_use]
    pub fn with_buffer(mut self, buffer: usize) -> Self {
        self.buffer = buffer.max(1);
        self
    }

    /// Watch `roots` recursively, yielding a debounced [`FsChangeStream`].
    ///
    /// Raw platform events are bridged onto a bounded channel, made cancellable
    /// with `cancel`, coalesced by [`rdebounce_batch`](rskit_stream::RskitStreamExt::rdebounce_batch)
    /// over the debounce window, and mapped to sorted, deduplicated
    /// [`FsChangeBatch`]es. If the platform watcher reports an error (typically a
    /// queue overflow) during a window, the resulting batch carries a rescan
    /// signal ([`FsChangeBatch::rescan_requested`]) so consumers can re-evaluate
    /// the tree instead of silently missing dropped changes. The stream is lazy —
    /// no task is spawned; it is driven by whoever polls it — and completes when
    /// `cancel` fires or the platform watcher stops. Dropping the returned stream
    /// stops the OS watcher.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorCode::InvalidInput`] when `roots` is empty; otherwise a
    /// typed error (cause preserved) classified from the platform failure —
    /// [`ErrorCode::NotFound`] when a root does not exist, [`ErrorCode::Forbidden`]
    /// when it cannot be accessed, [`ErrorCode::ServiceUnavailable`] when the OS
    /// watch limit is reached, or [`ErrorCode::Internal`] for other failures.
    pub fn watch(&self, roots: &[PathBuf], cancel: CancellationToken) -> AppResult<FsChangeStream> {
        if roots.is_empty() {
            return Err(AppError::invalid_input(
                "roots",
                "filesystem watch requires at least one root path",
            ));
        }

        let (raw_tx, raw_rx) = mpsc::channel::<RawEvent>(self.buffer);
        let mut watcher =
            notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
                // The platform callback runs on `notify`'s own OS thread, not a
                // runtime worker, so a blocking send is the correct backpressure
                // primitive here.
                match result {
                    Ok(event) => {
                        for path in event.paths {
                            if raw_tx.blocking_send(RawEvent::Changed(path)).is_err() {
                                break;
                            }
                        }
                    }
                    // A watcher error (typically a queue overflow) means change
                    // notifications may have been dropped; signal a rescan so the
                    // consumer re-evaluates the tree instead of silently missing
                    // changes.
                    Err(_) => {
                        let _ = raw_tx.blocking_send(RawEvent::Rescan);
                    }
                }
            })
            .map_err(|error| watch_error("create filesystem watcher", None, error))?;

        for root in roots {
            if let Err(error) = watcher.watch(root, RecursiveMode::Recursive) {
                // Reverse binding-order would drop `watcher` before `raw_rx` on this
                // early return, re-opening the macOS fsevent teardown deadlock that
                // `WatchedStream`'s field order guards against: dropping the watcher
                // joins the runloop thread, which may be parked in `blocking_send` on
                // a full channel. Close the receiver first so any parked send unblocks
                // (returns `Err`) before the watcher is dropped.
                drop(raw_rx);
                return Err(watch_error("watch path", Some(root), error));
            }
        }

        // Lazy pipeline: bounded raw source → cancellation → trailing-edge debounce
        // → coalesce into a batch. No spawned task owns the watcher; the wrapper
        // below keeps it alive for the stream's lifetime and drops it on teardown.
        let batches = from_channel(raw_rx)
            .take_until(cancel.cancelled_owned())
            .rdebounce_batch(self.debounce, MAX_BATCH_EVENTS)
            .map(batch_from_raw);

        Ok(Box::pin(WatchedStream {
            batches: Box::pin(batches),
            _watcher: watcher,
        }))
    }
}

/// Couples the platform watcher's lifetime to the debounced stream.
///
/// Field order is a correctness requirement, not cosmetic: Rust drops fields in
/// declaration order, so `batches` (which owns the raw-event receiver) must drop
/// **before** `_watcher`. On macOS the fsevent backend's `Drop` joins the runloop
/// thread that invokes our `blocking_send`; if that send is parked on a full
/// bounded channel, closing the receiver first releases it so the join can
/// complete. Dropping the watcher first would leave the send blocked and hang
/// teardown.
struct WatchedStream {
    batches: FsChangeStream,
    _watcher: RecommendedWatcher,
}

impl Stream for WatchedStream {
    type Item = FsChangeBatch;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.batches.as_mut().poll_next(cx)
    }
}

/// Map a `notify` error to a typed [`AppError`], preserving the cause and path.
fn watch_error(action: &str, path: Option<&Path>, error: notify::Error) -> AppError {
    let message = path.map_or_else(
        || format!("failed to {action}"),
        |path| format!("failed to {action} '{}'", path.display()),
    );
    AppError::new(watch_error_code(&error), message).with_cause(error)
}

/// Classify a `notify` error into a typed [`ErrorCode`] so watch failures carry
/// a meaningful status (and HTTP mapping) instead of a blanket `Internal`/500.
fn watch_error_code(error: &notify::Error) -> ErrorCode {
    match &error.kind {
        notify::ErrorKind::PathNotFound | notify::ErrorKind::WatchNotFound => ErrorCode::NotFound,
        notify::ErrorKind::InvalidConfig(_) => ErrorCode::InvalidInput,
        notify::ErrorKind::MaxFilesWatch => ErrorCode::ServiceUnavailable,
        notify::ErrorKind::Io(io) => match io.kind() {
            std::io::ErrorKind::NotFound => ErrorCode::NotFound,
            std::io::ErrorKind::PermissionDenied => ErrorCode::Forbidden,
            _ => ErrorCode::Internal,
        },
        notify::ErrorKind::Generic(_) => ErrorCode::Internal,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use futures::StreamExt as _;
    use rskit_stream::CancellationToken;

    use super::FsWatcher;
    use crate::TempDir;

    #[test]
    fn empty_roots_is_a_typed_input_error() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let result =
                FsWatcher::new(Duration::from_millis(50)).watch(&[], CancellationToken::new());
            let error = result.err().expect("empty roots must error");
            assert!(error.to_string().contains("at least one root"));
        });
    }

    #[test]
    fn watching_a_missing_root_is_a_typed_error() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let dir = TempDir::new().unwrap();
            let missing = dir.path().join("does-not-exist");
            let result = FsWatcher::new(Duration::from_millis(50))
                .watch(std::slice::from_ref(&missing), CancellationToken::new());
            let error = result.err().expect("missing root must error");
            assert!(!error.to_string().is_empty());
        });
    }

    // A valid root followed by a missing one must error on setup and return
    // promptly — the missing-root error path drops the raw receiver before the
    // watcher, so watcher teardown never deadlocks on a parked `blocking_send`.
    // A multi-thread runtime + timeout turns a hang into a test failure.
    #[test]
    fn a_missing_root_after_a_valid_one_errors_without_hanging() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let dir = TempDir::new().unwrap();
            let valid = dir.path().to_path_buf();
            let missing = dir.path().join("does-not-exist");
            let watch = || {
                FsWatcher::new(Duration::from_millis(50))
                    .watch(&[valid.clone(), missing.clone()], CancellationToken::new())
            };
            let result = tokio::time::timeout(Duration::from_secs(5), async { watch() })
                .await
                .expect("watch setup must not hang");
            assert!(result.is_err(), "a missing root must fail setup");
        });
    }

    #[test]
    fn notify_errors_map_to_meaningful_codes() {
        use super::watch_error_code;
        use rskit_errors::ErrorCode;
        assert_eq!(
            watch_error_code(&notify::Error::path_not_found()),
            ErrorCode::NotFound
        );
        assert_eq!(
            watch_error_code(&notify::Error::watch_not_found()),
            ErrorCode::NotFound
        );
        assert_eq!(
            watch_error_code(&notify::Error::new(notify::ErrorKind::MaxFilesWatch)),
            ErrorCode::ServiceUnavailable,
        );
        assert_eq!(
            watch_error_code(&notify::Error::io(std::io::Error::from(
                std::io::ErrorKind::NotFound
            ))),
            ErrorCode::NotFound,
        );
        assert_eq!(
            watch_error_code(&notify::Error::io(std::io::Error::from(
                std::io::ErrorKind::PermissionDenied
            ))),
            ErrorCode::Forbidden,
        );
        assert_eq!(
            watch_error_code(&notify::Error::generic("boom")),
            ErrorCode::Internal
        );
    }

    #[test]
    fn batch_from_raw_dedups_paths_and_flags_rescan() {
        use super::{RawEvent, batch_from_raw};
        let a = PathBuf::from("/repo/a.rs");
        let b = PathBuf::from("/repo/b.rs");
        let batch = batch_from_raw(vec![
            RawEvent::Changed(a.clone()),
            RawEvent::Changed(b.clone()),
            RawEvent::Changed(a.clone()),
            RawEvent::Rescan,
        ]);
        assert!(batch.rescan_requested());
        assert_eq!(
            batch.paths().iter().cloned().collect::<Vec<_>>(),
            vec![a, b]
        );
    }

    #[test]
    fn batch_from_raw_without_errors_does_not_request_rescan() {
        use super::{RawEvent, batch_from_raw};
        let batch = batch_from_raw(vec![RawEvent::Changed(PathBuf::from("/repo/a.rs"))]);
        assert!(!batch.rescan_requested());
        assert_eq!(batch.len(), 1);
    }

    #[test]
    fn batch_from_raw_rescan_only_is_not_empty() {
        use super::{RawEvent, batch_from_raw};
        let batch = batch_from_raw(vec![RawEvent::Rescan]);
        assert!(batch.rescan_requested());
        assert!(batch.paths().is_empty());
        assert!(!batch.is_empty());
    }

    // Real-filesystem wiring smoke test: a write under a watched root surfaces as
    // a batch. Uses a multi-thread runtime (the `notify` callback blocking-sends
    // from its own thread) and a generous timeout rather than asserting timing.
    #[test]
    fn a_write_under_a_watched_root_yields_a_batch() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let dir = TempDir::new().unwrap();
            let root = dir.path().to_path_buf();
            let cancel = CancellationToken::new();
            let mut stream = FsWatcher::new(Duration::from_millis(50))
                .watch(std::slice::from_ref(&root), cancel.clone())
                .unwrap();

            let file = dir.child("touched.txt").unwrap();
            tokio::fs::write(&file, b"hello").await.unwrap();

            let batch = tokio::time::timeout(Duration::from_secs(5), stream.next())
                .await
                .expect("a change batch should arrive")
                .expect("stream should yield a batch");
            assert!(batch.any(|path| path.ends_with("touched.txt")));

            cancel.cancel();
        });
    }
}
