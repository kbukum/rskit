use std::sync::Arc;

/// Callback invoked for line-oriented process output.
pub type OutputLineCallback = Arc<dyn Fn(&str) + Send + Sync + 'static>;

/// Callback invoked for raw process output bytes.
pub type OutputBytesCallback = Arc<dyn Fn(&[u8]) + Send + Sync + 'static>;

/// Optional callbacks for line-oriented process output.
#[derive(Clone, Default)]
pub struct OutputObserver {
    pub(in crate::runner) stdout_line: Option<OutputLineCallback>,
    pub(in crate::runner) stderr_line: Option<OutputLineCallback>,
    pub(in crate::runner) stdout_bytes: Option<OutputBytesCallback>,
    pub(in crate::runner) stderr_bytes: Option<OutputBytesCallback>,
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

    /// Observe raw stdout bytes.
    #[must_use]
    pub fn with_stdout_bytes(mut self, callback: impl Fn(&[u8]) + Send + Sync + 'static) -> Self {
        self.stdout_bytes = Some(Arc::new(callback));
        self
    }

    /// Observe raw stderr bytes.
    #[must_use]
    pub fn with_stderr_bytes(mut self, callback: impl Fn(&[u8]) + Send + Sync + 'static) -> Self {
        self.stderr_bytes = Some(Arc::new(callback));
        self
    }

    pub(crate) fn stdout_bytes_callback(&self) -> Option<OutputBytesCallback> {
        self.stdout_bytes.clone()
    }

    pub(crate) fn stderr_bytes_callback(&self) -> Option<OutputBytesCallback> {
        self.stderr_bytes.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use super::*;

    #[test]
    fn output_observer_builders_store_callbacks_and_debug_flags() {
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
                move |chunk| {
                    bytes.fetch_add(chunk.len(), Ordering::SeqCst);
                }
            })
            .with_stderr_bytes({
                let bytes = Arc::clone(&stderr_bytes);
                move |chunk| {
                    bytes.fetch_add(chunk.len(), Ordering::SeqCst);
                }
            });

        (observer.stdout_line.as_ref().unwrap())("out");
        (observer.stderr_line.as_ref().unwrap())("err");
        (observer.stdout_bytes.as_ref().unwrap())(b"stdout");
        (observer.stderr_bytes.as_ref().unwrap())(b"stderr");

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
