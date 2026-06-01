//! Process specification, I/O modes, and execution policy.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

use crate::runner::OutputObserver;

/// Default maximum retained bytes for each captured output stream.
pub const DEFAULT_MAX_OUTPUT_BYTES: usize = 1024 * 1024;

/// Command environment policy.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[non_exhaustive]
pub enum EnvPolicy {
    /// Inherit parent environment variables, then apply explicit overrides.
    Inherit,
    /// Start from an empty environment, then apply explicit variables.
    Empty,
}

/// What to execute as a subprocess.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ProcessSpec {
    /// Program name or path to execute.
    pub program: PathBuf,
    /// Command-line arguments.
    pub args: Vec<OsString>,
    /// Working directory for the process.
    pub dir: Option<PathBuf>,
    /// Environment variables to set.
    pub env: HashMap<String, String>,
    /// Environment inheritance policy.
    pub env_policy: EnvPolicy,
}

impl ProcessSpec {
    /// Create a new process spec with just a program name.
    #[must_use]
    pub fn new<P: Into<PathBuf>>(program: P) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            dir: None,
            env: HashMap::new(),
            env_policy: EnvPolicy::Inherit,
        }
    }

    /// Add a command-line argument.
    #[must_use]
    pub fn arg<S: Into<OsString>>(mut self, arg: S) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Add multiple command-line arguments.
    #[must_use]
    pub fn args<I>(mut self, args: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<OsString>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Set the working directory.
    #[must_use]
    pub fn dir<P: Into<PathBuf>>(mut self, dir: P) -> Self {
        self.dir = Some(dir.into());
        self
    }

    /// Set an environment variable.
    #[must_use]
    pub fn env<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Set multiple environment variables.
    #[must_use]
    pub fn envs<K: Into<String>, V: Into<String>, I: IntoIterator<Item = (K, V)>>(
        mut self,
        vars: I,
    ) -> Self {
        for (k, v) in vars {
            self.env.insert(k.into(), v.into());
        }
        self
    }

    /// Set the environment policy.
    #[must_use]
    pub fn env_policy(mut self, policy: EnvPolicy) -> Self {
        self.env_policy = policy;
        self
    }

    /// Start the process with an empty environment.
    #[must_use]
    pub fn empty_env(mut self) -> Self {
        self.env_policy = EnvPolicy::Empty;
        self
    }
}

/// Create a subprocess specification.
#[must_use]
pub fn command<P: Into<PathBuf>>(program: P) -> ProcessSpec {
    ProcessSpec::new(program)
}

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
}

/// Signal and process-tree termination policy.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[non_exhaustive]
pub struct SignalPolicy {
    /// Grace period to wait after graceful termination before killing.
    pub grace_period: Duration,
    /// Create a new process group/session where supported.
    pub create_process_group: bool,
    /// Terminate the process group rather than only the immediate child where supported.
    pub terminate_descendants: bool,
}

impl Default for SignalPolicy {
    fn default() -> Self {
        Self {
            grace_period: Duration::from_secs(5),
            create_process_group: true,
            terminate_descendants: true,
        }
    }
}

impl SignalPolicy {
    /// Set the graceful termination period before kill escalation.
    #[must_use]
    pub fn with_grace_period(mut self, grace_period: Duration) -> Self {
        self.grace_period = grace_period;
        self
    }

    /// Set whether processes are spawned into a new process group where supported.
    #[must_use]
    pub fn with_create_process_group(mut self, create_process_group: bool) -> Self {
        self.create_process_group = create_process_group;
        self
    }

    /// Set whether termination targets descendants through the process group.
    #[must_use]
    pub fn with_terminate_descendants(mut self, terminate_descendants: bool) -> Self {
        self.terminate_descendants = terminate_descendants;
        self
    }
}

/// Configuration for subprocess execution behavior.
#[derive(Debug, Clone)]
pub struct ProcessConfig {
    /// Overall timeout for the process. None means no timeout.
    pub timeout: Option<Duration>,
    /// Explicit I/O strategy.
    pub io: ProcessIo,
    /// Signal and process-tree termination policy.
    pub signal: SignalPolicy,
}

impl Default for ProcessConfig {
    fn default() -> Self {
        Self {
            timeout: Some(Duration::from_secs(30)),
            io: ProcessIo::default(),
            signal: SignalPolicy::default(),
        }
    }
}

impl ProcessConfig {
    /// Set the process timeout.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set the I/O strategy.
    #[must_use]
    pub fn with_io(mut self, io: ProcessIo) -> Self {
        self.io = io;
        self
    }

    /// Set the signal policy.
    #[must_use]
    pub fn with_signal_policy(mut self, signal: SignalPolicy) -> Self {
        self.signal = signal;
        self
    }

    /// Set the maximum retained bytes for captured or observed output.
    #[must_use]
    pub fn with_max_output_bytes(mut self, bytes: usize) -> Self {
        match &mut self.io {
            ProcessIo::Captured(io) => io.output.max_output_bytes = Some(bytes),
            ProcessIo::Observed(io) => io.output.max_output_bytes = Some(bytes),
            ProcessIo::Inherited(_) => {}
        }
        self
    }

    /// Disable output capture bounds for captured or observed output.
    #[must_use]
    pub fn with_unbounded_output(mut self) -> Self {
        match &mut self.io {
            ProcessIo::Captured(io) => io.output.max_output_bytes = None,
            ProcessIo::Observed(io) => io.output.max_output_bytes = None,
            ProcessIo::Inherited(_) => {}
        }
        self
    }

    /// Set stdin for the configured I/O mode.
    #[must_use]
    pub fn with_input(mut self, input: InputPolicy) -> Self {
        match &mut self.io {
            ProcessIo::Captured(io) => io.input = input,
            ProcessIo::Observed(io) => io.input = input,
            ProcessIo::Inherited(io) => io.input = input,
        }
        self
    }
}
