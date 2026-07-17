use std::path::{Path, PathBuf};

use rskit_errors::AppResult;
use rskit_process::{ProcessConfig, ProcessResult, ProcessSpec};

use crate::core::Executor;
use crate::error::GitError;

/// Git CLI command runner rooted at a repository.
pub struct GitCli {
    root: PathBuf,
}

impl GitCli {
    /// Creates a Git CLI runner rooted at the repository path.
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Returns the repository root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn run(&self, args: &[&str]) -> AppResult<Vec<u8>> {
        let output = self.run_result(args)?;
        if output.success() && !output.stdout_truncated && !output.stderr_truncated {
            Ok(output.stdout_bytes)
        } else {
            Err(Self::command_failed(args, output))
        }
    }

    pub(crate) fn run_result(&self, args: &[&str]) -> AppResult<ProcessResult> {
        let command = ProcessSpec::new("git")
            .args(args.iter().copied())
            .dir(&self.root)
            .env("GIT_TERMINAL_PROMPT", "0");
        let config = ProcessConfig::default().with_timeout(None);
        rskit_process::run(&command, &config)
    }

    #[allow(dead_code)]
    pub(crate) fn not_implemented<T>(&self, operation: &'static str) -> AppResult<T> {
        Err(GitError::NotImplemented { operation }.into())
    }

    pub(crate) fn command_failed(args: &[&str], output: ProcessResult) -> rskit_errors::AppError {
        GitError::CommandFailed {
            args: args.iter().map(|arg| (*arg).to_string()).collect(),
            exit_code: output.exit_code,
            stdout: output.stdout.trim().to_string(),
            stderr: output.stderr.trim().to_string(),
            stdout_truncated: output.stdout_truncated,
            stderr_truncated: output.stderr_truncated,
        }
        .into()
    }
}

impl Executor for GitCli {
    fn exec(&self, args: &[&str]) -> AppResult<Vec<u8>> {
        self.run(args)
    }
}
