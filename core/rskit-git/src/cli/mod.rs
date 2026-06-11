//! Git CLI implementation for command-oriented workflows.

pub mod auth;
mod manage;
mod read;
mod write;

use std::path::{Path, PathBuf};

use rskit_errors::AppResult;
use rskit_process::{ProcessConfig, ProcessResult, ProcessSpec};

use crate::core::Executor;
use crate::error::GitError;
use crate::types::Oid;

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

impl Executor for GitCli {
    fn exec(&self, args: &[&str]) -> AppResult<Vec<u8>> {
        self.run(args)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use rskit_process::ProcessResult;

    use super::*;

    #[test]
    fn new_exposes_root_and_executes_git_commands() {
        let root = rskit_testutil::test_workspace!("git-cli-root");
        let cli = GitCli::new(root.path().to_path_buf());

        assert_eq!(cli.root(), root.path());
        let version = cli.exec(&["--version"]).expect("git version runs");
        assert!(String::from_utf8_lossy(&version).contains("git version"));
    }

    #[test]
    fn command_failed_preserves_diagnostics_and_truncation_flags() {
        let output = ProcessResult::completed(
            Some(129),
            b"usage\n".to_vec(),
            b"bad args\n".to_vec(),
            false,
            false,
            Duration::ZERO,
            true,
            false,
        );

        let err = GitCli::command_failed(&["bad"], output);

        assert_eq!(err.code(), rskit_errors::ErrorCode::ExternalService);
        assert!(err.message().contains("external service error"));
    }

    #[test]
    fn parse_oid_rejects_wrong_length_and_invalid_hex() {
        assert!(parse_oid("abc").is_err());
        assert!(parse_oid("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz").is_err());
        assert_eq!(
            parse_oid("0000000000000000000000000000000000000001")
                .unwrap()
                .to_string(),
            "0000000000000000000000000000000000000001"
        );
    }

    #[test]
    fn not_implemented_maps_to_internal_error() {
        let root = rskit_testutil::test_workspace!("git-cli-not-implemented");
        let cli = GitCli::new(root.path().to_path_buf());

        let err = cli
            .not_implemented::<()>("future operation")
            .expect_err("operation is not implemented");

        assert_eq!(err.code(), rskit_errors::ErrorCode::InvalidInput);
    }
}
