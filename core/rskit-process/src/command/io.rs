//! Process I/O strategies and their capture / observation policies.

#[cfg(unix)]
use crate::pty::PtyIo;
use crate::runner::OutputObserver;

/// Default maximum retained bytes for each captured output stream.
pub const DEFAULT_MAX_OUTPUT_BYTES: usize = 1024 * 1024;
/// Standard input policy.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
#[non_exhaustive]
pub enum InputPolicy {
    /// Close stdin for the child process.
    #[default]
    Closed,
    /// Pipe predefined bytes to stdin, then close it.
    Bytes(Vec<u8>),
    /// Inherit stdin from the parent process.
    Inherit,
}

/// Output capture policy for pipe-backed modes.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct OutputPolicy {
    /// Whether stdout should be retained in `ProcessResult`.
    pub capture_stdout: bool,
    /// Whether stderr should be retained in `ProcessResult`.
    pub capture_stderr: bool,
    /// Maximum retained bytes for each captured stream. `None` means unbounded.
    pub max_output_bytes: Option<usize>,
}

impl Default for OutputPolicy {
    fn default() -> Self {
        Self::captured()
    }
}

impl OutputPolicy {
    /// Capture stdout and stderr with the default bounds.
    #[must_use]
    pub const fn captured() -> Self {
        Self {
            capture_stdout: true,
            capture_stderr: true,
            max_output_bytes: Some(DEFAULT_MAX_OUTPUT_BYTES),
        }
    }

    /// Do not retain stdout or stderr.
    #[must_use]
    pub const fn observe_only() -> Self {
        Self {
            capture_stdout: false,
            capture_stderr: false,
            max_output_bytes: Some(DEFAULT_MAX_OUTPUT_BYTES),
        }
    }

    /// Set the maximum retained bytes for each captured output stream.
    #[must_use]
    pub fn with_max_output_bytes(mut self, bytes: usize) -> Self {
        self.max_output_bytes = Some(bytes);
        self
    }

    /// Disable output capture bounds.
    #[must_use]
    pub fn with_unbounded_output(mut self) -> Self {
        self.max_output_bytes = None;
        self
    }
}

/// Pipe-backed deterministic capture mode.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct CapturedIo {
    /// Standard input policy.
    pub input: InputPolicy,
    /// Output capture policy.
    pub output: OutputPolicy,
}

impl CapturedIo {
    /// Create capture mode with default closed stdin and bounded stdout/stderr capture.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the stdin policy.
    #[must_use]
    pub fn with_input(mut self, input: InputPolicy) -> Self {
        self.input = input;
        self
    }

    /// Set the output policy.
    #[must_use]
    pub fn with_output(mut self, output: OutputPolicy) -> Self {
        self.output = output;
        self
    }
}

/// Pipe-backed live observation mode with optional capture.
#[derive(Debug, Clone, Default)]
pub struct ObservedIo {
    /// Standard input policy.
    pub input: InputPolicy,
    /// Output capture policy.
    pub output: OutputPolicy,
    /// Output callbacks.
    pub observer: OutputObserver,
}

impl ObservedIo {
    /// Create observed mode with the provided callbacks.
    #[must_use]
    pub fn new(observer: OutputObserver) -> Self {
        Self {
            observer,
            ..Self::default()
        }
    }

    /// Set the stdin policy.
    #[must_use]
    pub fn with_input(mut self, input: InputPolicy) -> Self {
        self.input = input;
        self
    }

    /// Set the output policy.
    #[must_use]
    pub fn with_output(mut self, output: OutputPolicy) -> Self {
        self.output = output;
        self
    }
}

/// Inherited terminal stdio mode.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct InheritedIo {
    /// Standard input policy. Defaults to inheriting parent stdin.
    pub input: InputPolicy,
}

impl Default for InheritedIo {
    fn default() -> Self {
        Self {
            input: InputPolicy::Inherit,
        }
    }
}

impl InheritedIo {
    /// Create inherited stdio mode.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the stdin policy while stdout/stderr remain inherited.
    #[must_use]
    pub fn with_input(mut self, input: InputPolicy) -> Self {
        self.input = input;
        self
    }
}

/// Process I/O strategy.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ProcessIo {
    /// Capture stdout/stderr separately through pipes.
    Captured(CapturedIo),
    /// Observe stdout/stderr live through pipes with optional capture.
    Observed(ObservedIo),
    /// Inherit parent terminal stdio.
    Inherited(InheritedIo),
    /// Run the child on a pseudoterminal so it renders as in a real terminal.
    ///
    /// stdout and stderr are merged into one stream; see [`PtyIo`]. Unix-only.
    #[cfg(unix)]
    Pty(PtyIo),
}

impl Default for ProcessIo {
    fn default() -> Self {
        Self::Captured(CapturedIo::default())
    }
}

impl ProcessIo {
    /// Create captured I/O mode.
    #[must_use]
    pub fn captured(io: CapturedIo) -> Self {
        Self::Captured(io)
    }

    /// Create observed I/O mode.
    #[must_use]
    pub fn observed(io: ObservedIo) -> Self {
        Self::Observed(io)
    }

    /// Create inherited I/O mode.
    #[must_use]
    pub fn inherited(io: InheritedIo) -> Self {
        Self::Inherited(io)
    }

    /// Create pseudoterminal I/O mode.
    #[cfg(unix)]
    #[must_use]
    pub fn pty(io: PtyIo) -> Self {
        Self::Pty(io)
    }
}
