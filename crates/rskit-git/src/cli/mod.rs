//! CLI backend for command-oriented git workflows.

pub mod auth;
mod manage;
mod read;
mod write;

use std::path::{Path, PathBuf};
use std::process::Command;

use rskit_errors::AppResult;

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
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.root)
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .map_err(|err| GitError::CommandFailed {
                args: args.iter().map(|arg| (*arg).to_string()).collect(),
                stderr: err.to_string(),
            })?;

        if output.status.success() {
            Ok(output.stdout)
        } else {
            Err(GitError::CommandFailed {
                args: args.iter().map(|arg| (*arg).to_string()).collect(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            }
            .into())
        }
    }

    #[allow(dead_code)]
    pub(crate) fn not_implemented<T>(&self, operation: &'static str) -> AppResult<T> {
        Err(GitError::NotImplemented { operation }.into())
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
