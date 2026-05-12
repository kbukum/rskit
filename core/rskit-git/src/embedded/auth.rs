//! `git2` authentication helpers.

use crate::auth::TransportAuth;
use crate::error::GitError;
use rskit_errors::AppResult;

/// Builds remote callbacks from the configured transport auth.
#[allow(dead_code)]
pub fn remote_callbacks(auth: Option<&TransportAuth>) -> AppResult<git2::RemoteCallbacks<'static>> {
    let mut callbacks = git2::RemoteCallbacks::new();
    match auth.cloned().unwrap_or_default() {
        TransportAuth::Default => {}
        TransportAuth::UsernamePassword { username, password } => {
            callbacks
                .credentials(move |_, _, _| git2::Cred::userpass_plaintext(&username, &password));
        }
        TransportAuth::Token { username, token } => {
            let username = username.unwrap_or_else(|| "git".to_string());
            callbacks.credentials(move |_, _, _| git2::Cred::userpass_plaintext(&username, &token));
        }
        TransportAuth::SshKey {
            username,
            public_key,
            private_key,
            passphrase,
        } => {
            callbacks.credentials(move |_, _, _| {
                git2::Cred::ssh_key(
                    &username,
                    public_key.as_deref(),
                    &private_key,
                    passphrase.as_deref(),
                )
            });
        }
        TransportAuth::SshAgent { username } => {
            callbacks.credentials(move |_, _, _| git2::Cred::ssh_key_from_agent(&username));
        }
    }
    Ok(callbacks)
}

/// Validates that the requested transport auth can be expressed for `git2`.
#[allow(dead_code)]
pub fn validate_transport(auth: &TransportAuth) -> AppResult<()> {
    match auth {
        TransportAuth::Default
        | TransportAuth::UsernamePassword { .. }
        | TransportAuth::Token { .. }
        | TransportAuth::SshKey { .. }
        | TransportAuth::SshAgent { .. } => Ok(()),
        #[allow(unreachable_patterns)]
        _ => Err(GitError::InvalidTransport {
            kind: "unsupported transport auth".to_string(),
        }
        .into()),
    }
}
