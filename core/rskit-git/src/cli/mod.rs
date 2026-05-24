//! CLI backend for command-oriented git workflows.

pub mod auth;
mod manage;
mod read;
mod write;

use std::path::{Path, PathBuf};

use rskit_errors::AppResult;
use rskit_process::{ProcessConfig, ProcessResult, command};

use crate::core::Executor;
use crate::error::GitError;
use crate::types::Oid;

/// CLI-backed repository helper.
pub struct Backend {
    root: PathBuf,
}

impl Backend {
    /// Creates a CLI backend rooted at the repository path.
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
        let command = command("git")
            .args(args.iter().copied())
            .dir(&self.root)
            .env("GIT_TERMINAL_PROMPT", "0");
        let config = ProcessConfig {
            timeout: None,
            ..ProcessConfig::default()
        };
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

pub(crate) fn parse_oid(hex: &str) -> AppResult<Oid> {
    let hex = hex.trim();
    if hex.len() != 40 {
        return Err(GitError::InvalidOid {
            value: hex.to_string(),
        }
        .into());
    }

    let mut bytes = [0u8; 20];
    for i in 0..20 {
        bytes[i] =
            u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).map_err(|_| GitError::InvalidOid {
                value: hex.to_string(),
            })?;
    }

    Ok(Oid::from_bytes(bytes))
}

impl Executor for Backend {
    fn exec(&self, args: &[&str]) -> AppResult<Vec<u8>> {
        self.run(args)
    }
}
