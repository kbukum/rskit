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
}
