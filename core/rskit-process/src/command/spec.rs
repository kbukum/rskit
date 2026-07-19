//! What to execute as a subprocess and how to build it.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::PathBuf;

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
