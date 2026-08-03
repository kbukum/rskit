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

    /// Signing is not supported by the selected backend.
    #[error("signing is not supported by the selected backend")]
    SigningNotSupported,

    /// A signing operation was requested but no signing key is configured.
    ///
    /// `git tag -s` resolves the signer from the explicit `user.signingkey` override or the repository/global `user.signingkey` config. The CLI backend preflights that effective key and returns this actionable configuration error instead of surfacing a raw signer failure.
    #[error("signing key is not configured: {key} is not set")]
    SigningKeyMissing {
        /// Missing signing-key config key (for example `user.signingkey`).
        key: String,
    },

    /// Git author/committer identity is not configured.
    ///
    /// libgit2 cannot build a default signature because `user.name` and/or
    /// `user.email` are unset in the effective git configuration. This is an
    /// actionable configuration problem, not an internal failure.
    #[error("git identity is not configured: {key} is not set")]
    IdentityMissing {
        /// Missing identity config key (for example `user.name` or `user.email`).
        key: String,
    },

    /// Unsupported transport configuration.
    #[error("invalid transport configuration: {kind}")]
    InvalidTransport {
        /// Transport kind that could not be applied.
        kind: String,
    },

    /// Network error during remote operations.
    #[error("network error: {0}")]
    Network(String),

    /// Authentication or authorization failure during a remote operation.
    ///
    /// libgit2 reports the transport-level failure (bad credentials, expired
    /// token, insufficient scope/permission) via an `Auth` code or an
    /// `Http`/`Ssh`/`Callback` class. This is an actionable access problem, not
    /// an internal failure, so its (credential-redacted) message is surfaced to
    /// the user.
    #[error("remote authentication failed: {message}")]
    RemoteAuth {
        /// Credential-redacted description of the authentication failure.
        message: String,
    },

    /// The remote rejected the push of one or more references.
    ///
    /// Covers a non-fast-forward update, a protected-branch/hook rejection, and
    /// any other server-reported ref rejection. The rejection is reported by
    /// the remote (not an internal failure), so the offending ref and the
    /// (credential-redacted) reason are surfaced to the user.
    #[error("remote rejected push to {refname}: {reason}")]
    PushRejected {
        /// Fully-qualified destination reference(s) the remote rejected; a
        /// comma-separated list when several refs were pushed together.
        refname: String,
        /// Credential-redacted rejection reason reported by the remote.
        reason: String,
    },

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
                "signing is not supported by the selected backend",
            ),
            GitError::SigningKeyMissing { key } => {
                AppError::invalid_input("sign", format!("{key} is not configured")).hint(
                    "Configure a signing key with `git config user.signingkey <key>` (for \
                     example an OpenPGP key id, SSH private-key path, X.509 certificate, or \
                     signer-specific key reference; add --global to apply it for every \
                     repository), then retry the signed release tag.",
                )
            }
            GitError::IdentityMissing { key } => {
                AppError::invalid_input("git identity", format!("{key} is not configured"))
                    .hint(
                        "Set it with `git config user.name \"…\"` and \
                         `git config user.email \"…\"` (add --global to apply it for every repository).",
                    )
            }
            GitError::InvalidTransport { kind } => AppError::invalid_input("transport", kind),
            GitError::Network(message) => {
                AppError::external_service("git", io::Error::other(message))
            }
            GitError::RemoteAuth { message } => {
                AppError::unauthorized(format!("git remote authentication failed: {message}"))
                    .hint(
                        "Check the remote credentials and that the token/key grants push access \
                         to this repository (e.g. on GitHub, a fine-grained token needs the \
                         `contents: write` permission).",
                    )
            }
            GitError::PushRejected { refname, reason } => {
                AppError::conflict(format!("remote rejected push to {refname}: {reason}")).hint(
                    "The remote rejected the update. Integrate remote changes (fetch and rebase) \
                     for a non-fast-forward, or, if the branch is protected, land the commit \
                     through a pull request and push tags only.",
                )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_errors_map_to_actionable_app_error_codes() {
        let cases = [
            (
                GitError::NotFound {
                    path: PathBuf::from("repo"),
                },
                ErrorCode::NotFound,
            ),
            (
                GitError::RefNotFound {
                    refname: "main".to_string(),
                },
                ErrorCode::NotFound,
            ),
            (
                GitError::RemoteNotFound {
                    name: "origin".to_string(),
                },
                ErrorCode::NotFound,
            ),
            (
                GitError::ConfigNotFound {
                    key: "user.name".to_string(),
                },
                ErrorCode::NotFound,
            ),
            (
                GitError::AmbiguousRef {
                    refname: "feature".to_string(),
                },
                ErrorCode::InvalidInput,
            ),
            (
                GitError::AlreadyExists {
                    kind: "branch",
                    name: "main".to_string(),
                },
                ErrorCode::AlreadyExists,
            ),
            (
                GitError::CheckedOutBranch {
                    name: "main".to_string(),
                },
                ErrorCode::Conflict,
            ),
            (
                GitError::Conflict {
                    path: PathBuf::from("src/lib.rs"),
                },
                ErrorCode::Conflict,
            ),
            (GitError::DetachedHead, ErrorCode::InvalidInput),
            (
                GitError::InvalidLineRange { start: 5, end: 3 },
                ErrorCode::InvalidInput,
            ),
            (
                GitError::InvalidPath {
                    path: "../outside".to_string(),
                },
                ErrorCode::InvalidInput,
            ),
            (
                GitError::InvalidConfigKey {
                    key: "bad key".to_string(),
                },
                ErrorCode::InvalidInput,
            ),
            (
                GitError::NoMergeBase {
                    a: "a".to_string(),
                    b: "b".to_string(),
                },
                ErrorCode::NotFound,
            ),
            (GitError::SigningNotSupported, ErrorCode::InvalidInput),
            (
                GitError::SigningKeyMissing {
                    key: "user.signingkey".to_string(),
                },
                ErrorCode::InvalidInput,
            ),
            (
                GitError::IdentityMissing {
                    key: "user.name".to_string(),
                },
                ErrorCode::InvalidInput,
            ),
            (
                GitError::InvalidTransport {
                    kind: "ssh".to_string(),
                },
                ErrorCode::InvalidInput,
            ),
            (
                GitError::Network("offline".to_string()),
                ErrorCode::ExternalService,
            ),
            (
                GitError::RemoteAuth {
                    message: "401 Unauthorized".to_string(),
                },
                ErrorCode::Unauthorized,
            ),
            (
                GitError::PushRejected {
                    refname: "refs/heads/main".to_string(),
                    reason: "protected branch".to_string(),
                },
                ErrorCode::Conflict,
            ),
            (
                GitError::CommandFailed {
                    args: vec!["status".to_string()],
                    exit_code: Some(128),
                    stdout: "partial stdout".to_string(),
                    stderr: "fatal".to_string(),
                    stdout_truncated: true,
                    stderr_truncated: false,
                },
                ErrorCode::ExternalService,
            ),
            (
                GitError::InvalidOid {
                    value: "not-a-sha".to_string(),
                },
                ErrorCode::InvalidInput,
            ),
            (
                GitError::NotImplemented { operation: "sign" },
                ErrorCode::InvalidInput,
            ),
            (
                GitError::Internal(git2::Error::from_str("git2 failed")),
                ErrorCode::Internal,
            ),
        ];

        for (git_error, expected) in cases {
            let app_error = AppError::from(git_error);
            assert_eq!(app_error.code(), expected);
        }
    }

    #[test]
    fn command_failure_message_includes_diagnostics() {
        let app_error = AppError::from(GitError::CommandFailed {
            args: vec!["push".to_string(), "origin".to_string()],
            exit_code: Some(1),
            stdout: "hint".to_string(),
            stderr: "denied".to_string(),
            stdout_truncated: false,
            stderr_truncated: true,
        });

        let detail = app_error
            .cause()
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| app_error.message().to_string());
        assert!(detail.contains("push origin"));
        assert!(detail.contains("denied"));
        assert!(detail.contains("exit code: 1"));
        assert!(detail.contains("stderr_truncated: true"));
        assert!(detail.contains("stdout: hint"));
    }

    #[test]
    fn identity_missing_message_is_actionable() {
        let app_error = AppError::from(GitError::IdentityMissing {
            key: "user.email".to_string(),
        });

        assert_eq!(app_error.code(), ErrorCode::InvalidInput);
        let message = app_error.message();
        assert!(message.contains("user.email"));
        assert!(message.contains("git config user.name"));
        assert!(message.contains("git config user.email"));
    }

    #[test]
    fn remote_auth_message_is_actionable() {
        let app_error = AppError::from(GitError::RemoteAuth {
            message: "403 Forbidden".to_string(),
        });

        assert_eq!(app_error.code(), ErrorCode::Unauthorized);
        assert!(app_error.message().contains("403 Forbidden"));
    }

    #[test]
    fn push_rejected_message_names_ref_and_reason() {
        let app_error = AppError::from(GitError::PushRejected {
            refname: "refs/heads/main".to_string(),
            reason: "protected branch hook declined".to_string(),
        });

        assert_eq!(app_error.code(), ErrorCode::Conflict);
        let message = app_error.message();
        assert!(message.contains("refs/heads/main"));
        assert!(message.contains("protected branch hook declined"));
    }
}
