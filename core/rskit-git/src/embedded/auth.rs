//! `git2` authentication helpers.

use crate::auth::TransportAuth;
use rskit_errors::AppResult;

/// Builds remote callbacks from the configured transport auth.
///
/// Secret material is exposed via [`SecretString::expose`](rskit_util::SecretString::expose)
/// only here, at the point it is handed to `git2::Cred`, so plaintext never
/// escapes onto the repository handle or into logs.
pub fn remote_callbacks(auth: Option<&TransportAuth>) -> AppResult<git2::RemoteCallbacks<'static>> {
    let mut callbacks = git2::RemoteCallbacks::new();
    match auth.cloned().unwrap_or_default() {
        TransportAuth::Default => {}
        TransportAuth::UsernamePassword { username, password } => {
            callbacks.credentials(move |_, _, _| {
                git2::Cred::userpass_plaintext(&username, password.expose())
            });
        }
        TransportAuth::Token { username, token } => {
            let username = username.unwrap_or_else(|| "x-access-token".to_string());
            callbacks.credentials(move |_, _, _| {
                git2::Cred::userpass_plaintext(&username, token.expose())
            });
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
                    passphrase.as_ref().map(rskit_util::SecretString::expose),
                )
            });
        }
        TransportAuth::SshAgent { username } => {
            callbacks.credentials(move |_, _, _| git2::Cred::ssh_key_from_agent(&username));
        }
    }
    Ok(callbacks)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use rskit_util::SecretString;

    use super::*;

    #[test]
    fn builds_callbacks_for_all_supported_transport_auth_variants() {
        let variants = [
            TransportAuth::Default,
            TransportAuth::UsernamePassword {
                username: "user".to_string(),
                password: SecretString::new("password"),
            },
            TransportAuth::Token {
                username: None,
                token: SecretString::new("token"),
            },
            TransportAuth::SshKey {
                username: "git".to_string(),
                public_key: None,
                private_key: PathBuf::from("id_ed25519"),
                passphrase: Some(SecretString::new("passphrase")),
            },
            TransportAuth::SshAgent {
                username: "git".to_string(),
            },
        ];

        for auth in variants {
            let _callbacks =
                remote_callbacks(Some(&auth)).expect("supported auth variant builds callbacks");
        }
    }

    #[test]
    fn remote_callbacks_defaults_when_auth_is_absent() {
        let _callbacks = remote_callbacks(None).expect("default callbacks build");
    }
}
