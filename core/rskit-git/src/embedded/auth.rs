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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn validates_all_supported_transport_auth_variants() {
        let variants = [
            TransportAuth::Default,
            TransportAuth::UsernamePassword {
                username: "user".to_string(),
                password: "password".to_string(),
            },
            TransportAuth::Token {
                username: None,
                token: "token".to_string(),
            },
            TransportAuth::SshKey {
                username: "git".to_string(),
                public_key: None,
                private_key: PathBuf::from("id_ed25519"),
                passphrase: Some("passphrase".to_string()),
            },
            TransportAuth::SshAgent {
                username: "git".to_string(),
            },
        ];

        for auth in variants {
            validate_transport(&auth).expect("supported auth variant validates");
            let _callbacks =
                remote_callbacks(Some(&auth)).expect("supported auth variant builds callbacks");
        }
    }

    #[test]
    fn remote_callbacks_defaults_when_auth_is_absent() {
        let _callbacks = remote_callbacks(None).expect("default callbacks build");
    }
}
