//! Git-specific error types.

use std::io;
use std::path::PathBuf;

use rskit_errors::{AppError, ErrorCode};

/// Git domain errors.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum GitError {
    /// Repository not found at path.
    #[error("repository not found at {path}")]
    NotFound {
        /// Filesystem path that did not contain a git repository.
        path: PathBuf,
    },

    /// Git reference not found.
    #[error("ref not found: {refname}")]
    RefNotFound {
        /// Missing reference name.
        refname: String,
    },

    /// Remote not found.
    #[error("remote not found: {name}")]
    RemoteNotFound {
        /// Missing remote name.
        name: String,
    },

    /// Config key not found.
    #[error("config key not found: {key}")]
    ConfigNotFound {
        /// Missing config key.
        key: String,
    },

    /// Ambiguous git reference.
    #[error("ambiguous ref: {refname}")]
    AmbiguousRef {
        /// Reference name that resolved to multiple candidates.
        refname: String,
    },

    /// Existing resource conflict.
    #[error("{kind} already exists: {name}")]
    AlreadyExists {
        /// Existing resource kind.
        kind: &'static str,
        /// Existing resource name.
        name: String,
    },

    /// Attempted to delete the currently checked out branch.
    #[error("branch is currently checked out: {name}")]
    CheckedOutBranch {
        /// Branch name.
        name: String,
    },

    /// Merge conflict in a file.
    #[error("merge conflict in {path}")]
    Conflict {
        /// Path containing merge conflict markers or index conflicts.
        path: PathBuf,
    },

    /// HEAD is detached (not pointing to a branch).
    #[error("detached HEAD")]
    DetachedHead,

    /// Invalid blame line range.
    #[error("invalid line range: {start}..{end}")]
    InvalidLineRange {
        /// One-based starting line.
        start: usize,
        /// One-based ending line.
        end: usize,
    },

    /// Invalid repository path.
    #[error("invalid path: {path}")]
    InvalidPath {
        /// Invalid repository-relative path.
        path: String,
    },

    /// Invalid git config key.
    #[error("invalid config key: {key}")]
    InvalidConfigKey {
        /// Invalid git config key.
        key: String,
    },

    /// No merge base exists between the two commits.
    #[error("no merge base found between {a} and {b}")]
    NoMergeBase {
        /// First commit reference.
        a: String,
        /// Second commit reference.
        b: String,
    },

    /// Commit signing is not supported by the selected backend.
    #[error("commit signing is not supported by the selected backend")]
    SigningNotSupported,

    /// Unsupported transport configuration.
    #[error("invalid transport configuration: {kind}")]
    InvalidTransport {
        /// Transport kind that could not be applied.
        kind: String,
    },

    /// Network error during remote operations.
    #[error("network error: {0}")]
    Network(String),

    /// Git CLI command failure.
    #[error("git CLI command failed: git {args:?}: {stderr}")]
    CommandFailed {
        /// CLI arguments that were attempted.
        args: Vec<String>,
        /// Process exit code, if available.
        exit_code: Option<i32>,
        /// Standard output (may contain conflict diagnostics).
        stdout: String,
        /// Standard error output.
        stderr: String,
        /// Whether stdout exceeded the configured capture limit.
        stdout_truncated: bool,
        /// Whether stderr exceeded the configured capture limit.
        stderr_truncated: bool,
    },

    /// Invalid object ID string.
    #[error("invalid object ID: {value}")]
    InvalidOid {
        /// Invalid hex value.
        value: String,
    },

    /// Operation is intentionally not implemented.
    #[error("git operation not implemented: {operation}")]
    NotImplemented {
        /// Unimplemented operation name.
        operation: &'static str,
    },

    /// Internal git2 error.
    #[error(transparent)]
    Internal(#[from] git2::Error),
}

impl From<GitError> for AppError {
    fn from(error: GitError) -> Self {
        match error {
            GitError::NotFound { path } => {
                let display = path.display().to_string();
                AppError::not_found("repository", Some(&display))
            }
            GitError::RefNotFound { refname } => AppError::not_found("ref", Some(&refname)),
            GitError::RemoteNotFound { name } => AppError::not_found("remote", Some(&name)),
            GitError::ConfigNotFound { key } => AppError::not_found("config", Some(&key)),
            GitError::AmbiguousRef { refname } => {
                AppError::invalid_input("ref", format!("ambiguous ref: {refname}"))
            }
            GitError::AlreadyExists { kind, name } => {
                AppError::already_exists(format!("{kind} '{name}'"))
            }
            GitError::CheckedOutBranch { name } => {
                AppError::conflict(format!("branch is currently checked out: {name}"))
            }
            GitError::Conflict { path } => {
                AppError::conflict(format!("merge conflict in {}", path.display()))
            }
            GitError::DetachedHead => AppError::invalid_input("HEAD", "detached HEAD"),
            GitError::InvalidLineRange { start, end } => {
                AppError::invalid_input("line range", format!("{start}..{end}"))
            }
            GitError::InvalidPath { path } => AppError::invalid_input("path", path),
            GitError::InvalidConfigKey { key } => AppError::invalid_input("key", key),
            GitError::NoMergeBase { a, b } => {
                AppError::not_found("merge base", Some(&format!("{a}..{b}")))
            }
            GitError::SigningNotSupported => AppError::invalid_input(
                "sign",
                "commit signing is not supported by the selected backend",
            ),
            GitError::InvalidTransport { kind } => AppError::invalid_input("transport", kind),
            GitError::Network(message) => {
                AppError::external_service("git", io::Error::other(message))
            }
            GitError::CommandFailed {
                args,
                exit_code,
                stdout,
                stderr,
                stdout_truncated,
                stderr_truncated,
            } => {
                let mut detail = format!("git {}: {}", args.join(" "), stderr);
                if let Some(exit_code) = exit_code {
                    detail.push_str(&format!(" (exit code: {exit_code})"));
                }
                if stdout_truncated || stderr_truncated {
                    detail.push_str(&format!(
                        " (stdout_truncated: {stdout_truncated}, stderr_truncated: {stderr_truncated})"
                    ));
                }
                if !stdout.is_empty() {
                    detail.push_str("\nstdout: ");
                    detail.push_str(&stdout);
                }
                AppError::external_service("git", io::Error::other(detail))
            }
            GitError::InvalidOid { value } => AppError::invalid_input("oid", value),
            GitError::NotImplemented { operation } => AppError::new(
                ErrorCode::InvalidInput,
                format!("git operation not supported: {operation}"),
            ),
            GitError::Internal(inner) => AppError::internal(inner),
        }
    }
}
