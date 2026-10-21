use std::{future::Future, pin::Pin, sync::Arc};

use crate::AppResult;

/// Callback invoked for line-oriented process output.
///
/// Line callbacks run inline on the async reader and must not block.
pub type OutputLineCallback = Arc<dyn Fn(&str) + Send + Sync + 'static>;

/// Callback invoked for raw process output bytes on a dedicated reader thread.
///
/// Persistent processes read on OS threads, so this callback may apply bounded blocking backpressure. Delivery errors propagate through process completion.
pub type OutputBytesCallback = Arc<dyn Fn(&[u8]) -> AppResult<()> + Send + Sync + 'static>;

/// Future returned by an [`OutputBytesSink`] for one chunk.
pub type OutputBytesDelivery = Pin<Box<dyn Future<Output = AppResult<()>> + Send + 'static>>;

/// Async consumer of raw process output bytes.
///
/// The reader awaits each delivery before reading more, so a slow consumer applies backpressure: the child blocks on a full pipe instead of output being dropped or buffered without bound. Delivery errors propagate through process completion; incomplete delivery after the post-exit grace period returns a timeout error. When the lifecycle policy owns descendants, a group member still holding the pipe at that point is ended first, so its output closes and delivery can finish.
pub type OutputBytesSink = Arc<dyn Fn(Vec<u8>) -> OutputBytesDelivery + Send + Sync + 'static>;

/// Optional observers for process output.
#[derive(Clone, Default)]
pub struct OutputObserver {
    pub(in crate::runner) stdout_line: Option<OutputLineCallback>,
    pub(in crate::runner) stderr_line: Option<OutputLineCallback>,
    pub(in crate::runner) stdout_bytes: Option<OutputBytesSink>,
    pub(in crate::runner) stderr_bytes: Option<OutputBytesSink>,
}

impl std::fmt::Debug for OutputObserver {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OutputObserver")
            .field("stdout_line", &self.stdout_line.is_some())
            .field("stderr_line", &self.stderr_line.is_some())
            .field("stdout_bytes", &self.stdout_bytes.is_some())
            .field("stderr_bytes", &self.stderr_bytes.is_some())
            .finish()
    }
}

impl OutputObserver {
    /// Create an observer without callbacks.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Observe each stdout line.
    #[must_use]
    pub fn with_stdout_line(mut self, callback: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.stdout_line = Some(Arc::new(callback));
        self
    }

    /// Observe each stderr line.
    #[must_use]
    pub fn with_stderr_line(mut self, callback: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.stderr_line = Some(Arc::new(callback));
        self
    }

    /// Deliver raw stdout bytes to an async sink; the reader awaits each delivery, so a slow sink backpressures the child instead of losing output.
    #[must_use]
    pub fn with_stdout_bytes<F, Fut>(mut self, sink: F) -> Self
    where
        F: Fn(Vec<u8>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = AppResult<()>> + Send + 'static,
    {
        self.stdout_bytes = Some(bytes_sink(sink));
        self
    }

    /// Deliver raw stderr bytes to an async sink; the reader awaits each delivery, so a slow sink backpressures the child instead of losing output.
    #[must_use]
    pub fn with_stderr_bytes<F, Fut>(mut self, sink: F) -> Self
    where
        F: Fn(Vec<u8>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = AppResult<()>> + Send + 'static,
    {
        self.stderr_bytes = Some(bytes_sink(sink));
        self
    }
}

fn bytes_sink<F, Fut>(sink: F) -> OutputBytesSink
where
    F: Fn(Vec<u8>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = AppResult<()>> + Send + 'static,
{
    Arc::new(move |chunk| Box::pin(sink(chunk)))
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use super::*;

    #[tokio::test]
    async fn output_observer_builders_store_callbacks_and_debug_flags() {
        let stdout_lines = Arc::new(AtomicUsize::new(0));
        let stderr_lines = Arc::new(AtomicUsize::new(0));
        let stdout_bytes = Arc::new(AtomicUsize::new(0));
        let stderr_bytes = Arc::new(AtomicUsize::new(0));

        let observer = OutputObserver::new()
            .with_stdout_line({
                let calls = Arc::clone(&stdout_lines);
                move |_| {
                    calls.fetch_add(1, Ordering::SeqCst);
                }
            })
            .with_stderr_line({
                let calls = Arc::clone(&stderr_lines);
                move |_| {
                    calls.fetch_add(1, Ordering::SeqCst);
                }
            })
            .with_stdout_bytes({
                let bytes = Arc::clone(&stdout_bytes);
                move |chunk: Vec<u8>| {
                    bytes.fetch_add(chunk.len(), Ordering::SeqCst);
                    async { Ok(()) }
                }
            })
            .with_stderr_bytes({
                let bytes = Arc::clone(&stderr_bytes);
                move |chunk: Vec<u8>| {
                    bytes.fetch_add(chunk.len(), Ordering::SeqCst);
                    async { Ok(()) }
                }
            });

        (observer.stdout_line.as_ref().unwrap())("out");
        (observer.stderr_line.as_ref().unwrap())("err");
        (observer.stdout_bytes.as_ref().unwrap())(b"stdout".to_vec())
            .await
            .unwrap();
        (observer.stderr_bytes.as_ref().unwrap())(b"stderr".to_vec())
            .await
            .unwrap();

        assert_eq!(stdout_lines.load(Ordering::SeqCst), 1);
        assert_eq!(stderr_lines.load(Ordering::SeqCst), 1);
        assert_eq!(stdout_bytes.load(Ordering::SeqCst), 6);
        assert_eq!(stderr_bytes.load(Ordering::SeqCst), 6);

        let debug = format!("{observer:?}");
        assert!(debug.contains("stdout_line: true"));
        assert!(debug.contains("stderr_line: true"));
        assert!(debug.contains("stdout_bytes: true"));
        assert!(debug.contains("stderr_bytes: true"));
    }
}
