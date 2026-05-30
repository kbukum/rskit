use std::sync::Arc;

/// Callback invoked for line-oriented process output.
pub type OutputLineCallback = Arc<dyn Fn(&str) + Send + Sync + 'static>;

/// Optional callbacks for line-oriented process output.
#[derive(Clone, Default)]
pub struct OutputObserver {
    pub(in crate::runner) stdout_line: Option<OutputLineCallback>,
    pub(in crate::runner) stderr_line: Option<OutputLineCallback>,
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
}
