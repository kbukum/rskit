//! Command configuration for subprocess execution.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// Default maximum retained bytes for each captured output stream.
pub const DEFAULT_MAX_OUTPUT_BYTES: usize = 1024 * 1024;

/// Configuration for subprocess execution behavior.
#[derive(Debug, Clone)]
pub struct ProcessConfig {
    /// Overall timeout for the process. None means no timeout.
    pub timeout: Option<Duration>,
    /// Grace period to wait after SIGTERM before sending SIGKILL.
    /// Defaults to 5 seconds.
    pub grace_period: Duration,
    /// Whether to capture stdout and stderr. Defaults to true.
    pub capture_output: bool,
    /// Whether to inherit parent process environment variables. Defaults to true.
    pub inherit_env: bool,
    /// Maximum number of bytes to retain for each captured output stream.
    /// When `None`, output capture is unbounded.
    pub max_output_bytes: Option<usize>,
}

impl Default for ProcessConfig {
    fn default() -> Self {
        Self {
            timeout: Some(Duration::from_secs(30)),
            grace_period: Duration::from_secs(5),
            capture_output: true,
            inherit_env: true,
            max_output_bytes: Some(DEFAULT_MAX_OUTPUT_BYTES),
        }
    }
}

impl ProcessConfig {
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

/// Command to execute as a subprocess.
#[derive(Debug, Clone)]
pub struct Command {
    /// Program name or path to execute.
    pub program: PathBuf,
    /// Command-line arguments.
    pub args: Vec<String>,
    /// Working directory for the process.
    pub dir: Option<PathBuf>,
    /// Environment variables to set (may be merged with parent env).
    pub env: HashMap<String, String>,
    /// Standard input data to pipe to the process.
    pub stdin: Option<Vec<u8>>,
    /// When true, start from an empty environment before applying `env`.
    pub scrub_env: bool,
}

impl Command {
    /// Create a new command with just a program name.
    #[must_use]
    pub fn new<P: Into<PathBuf>>(program: P) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            dir: None,
            env: HashMap::new(),
            stdin: None,
            scrub_env: false,
        }
    }

    /// Add a command-line argument.
    #[must_use]
    pub fn arg<S: Into<String>>(mut self, arg: S) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Add multiple command-line arguments.
    #[must_use]
    pub fn args<I>(mut self, args: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<String>,
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

    /// Set stdin data to be piped to the process.
    #[must_use]
    pub fn stdin<B: Into<Vec<u8>>>(mut self, stdin: B) -> Self {
        self.stdin = Some(stdin.into());
        self
    }

    /// Start the command with an empty environment.
    #[must_use]
    pub fn scrub_env(mut self) -> Self {
        self.scrub_env = true;
        self
    }
}

/// Create a subprocess command.
#[must_use]
pub fn command<P: Into<PathBuf>>(program: P) -> Command {
    Command::new(program)
}
