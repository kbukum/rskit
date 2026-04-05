//! Command configuration for subprocess execution.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

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
}

impl Default for ProcessConfig {
    fn default() -> Self {
        Self {
            timeout: Some(Duration::from_secs(30)),
            grace_period: Duration::from_secs(5),
            capture_output: true,
            inherit_env: true,
        }
    }
}

/// Command to execute as a subprocess.
#[derive(Debug, Clone)]
pub struct Command {
    /// Program name or path to execute.
    pub program: String,
    /// Command-line arguments.
    pub args: Vec<String>,
    /// Working directory for the process.
    pub dir: Option<PathBuf>,
    /// Environment variables to set (may be merged with parent env).
    pub env: HashMap<String, String>,
    /// Standard input data to pipe to the process.
    pub stdin: Option<Vec<u8>>,
}

impl Command {
    /// Create a new command with just a program name.
    ///
    /// # Arguments
    ///
    /// * `program` - The executable name or path
    ///
    /// # Example
    ///
    /// ```
    /// use rskit_process::Command;
    ///
    /// let cmd = Command::new("echo");
    /// ```
    pub fn new<S: Into<String>>(program: S) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            dir: None,
            env: HashMap::new(),
            stdin: None,
        }
    }

    /// Add a command-line argument.
    ///
    /// # Example
    ///
    /// ```
    /// use rskit_process::Command;
    ///
    /// let cmd = Command::new("echo").arg("hello").arg("world");
    /// ```
    pub fn arg<S: Into<String>>(mut self, arg: S) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Add multiple command-line arguments.
    ///
    /// # Example
    ///
    /// ```
    /// use rskit_process::Command;
    ///
    /// let cmd = Command::new("ls").args(vec!["-l", "-a"]);
    /// ```
    pub fn args<I>(mut self, args: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<String>,
    {
        self.args.extend(args.into_iter().map(|a| a.into()));
        self
    }

    /// Set the working directory.
    ///
    /// # Example
    ///
    /// ```
    /// use rskit_process::Command;
    /// use std::path::PathBuf;
    ///
    /// let cmd = Command::new("ls").dir("/tmp");
    /// ```
    pub fn dir<P: Into<PathBuf>>(mut self, dir: P) -> Self {
        self.dir = Some(dir.into());
        self
    }

    /// Set an environment variable.
    ///
    /// # Example
    ///
    /// ```
    /// use rskit_process::Command;
    ///
    /// let cmd = Command::new("printenv").env("MY_VAR", "my_value");
    /// ```
    pub fn env<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Set multiple environment variables.
    ///
    /// # Example
    ///
    /// ```
    /// use rskit_process::Command;
    /// use std::collections::HashMap;
    ///
    /// let mut env = HashMap::new();
    /// env.insert("VAR1".to_string(), "value1".to_string());
    /// env.insert("VAR2".to_string(), "value2".to_string());
    /// let cmd = Command::new("sh").envs(env);
    /// ```
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
    ///
    /// # Example
    ///
    /// ```
    /// use rskit_process::Command;
    ///
    /// let cmd = Command::new("cat").stdin(b"hello world".to_vec());
    /// ```
    pub fn stdin<B: Into<Vec<u8>>>(mut self, stdin: B) -> Self {
        self.stdin = Some(stdin.into());
        self
    }
}
