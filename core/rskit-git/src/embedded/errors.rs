use crate::error::GitError;

pub(crate) fn map_head_error(err: git2::Error) -> GitError {
    if err.code() == git2::ErrorCode::UnbornBranch || err.code() == git2::ErrorCode::NotFound {
        GitError::RefNotFound {
            refname: "HEAD".to_string(),
        }
    } else {
        GitError::Internal(err)
    }
}

pub(crate) fn map_remote_error(err: git2::Error) -> GitError {
    if err.class() == git2::ErrorClass::Net {
        GitError::Network(redact_url_credentials(err.message()))
    } else if is_auth_error(&err) {
        GitError::RemoteAuth {
            message: redact_url_credentials(err.message()),
        }
    } else {
        GitError::Internal(err)
    }
}

/// Classify a `git2` error raised by a push whose per-ref report did not already
/// surface a rejection.
///
/// Auth/permission failures (an `Auth` code or an `Http`/`Ssh`/`Callback` class)
/// become [`GitError::RemoteAuth`]; a non-fast-forward or other reference
/// rejection becomes [`GitError::PushRejected`] naming the destination
/// ref(s); genuinely internal git2 errors keep the opaque
/// [`GitError::Internal`] contract. Remote messages are surfaced with any
/// credentials in an embedded URL redacted.
pub(crate) fn map_push_error(err: git2::Error, refspecs: &[String]) -> GitError {
    if err.class() == git2::ErrorClass::Net {
        GitError::Network(redact_url_credentials(err.message()))
    } else if is_auth_error(&err) {
        GitError::RemoteAuth {
            message: redact_url_credentials(err.message()),
        }
    } else if is_ref_rejection(&err) {
        GitError::PushRejected {
            refname: destination_refs(refspecs),
            reason: redact_url_credentials(err.message()),
        }
    } else {
        GitError::Internal(err)
    }
}

/// Whether a `git2` remote error is an authentication/authorization failure.
fn is_auth_error(err: &git2::Error) -> bool {
    err.code() == git2::ErrorCode::Auth
        || matches!(
            err.class(),
            git2::ErrorClass::Http | git2::ErrorClass::Ssh | git2::ErrorClass::Callback
        )
}

/// Whether a `git2` push error is a server/reference rejection (non-fast-forward
/// or another refused ref update).
fn is_ref_rejection(err: &git2::Error) -> bool {
    err.code() == git2::ErrorCode::NotFastForward || err.class() == git2::ErrorClass::Reference
}

/// Join the destination refs of the pushed refspecs for a rejection message.
///
/// A refspec is `src:dst`; the destination side names the rejected remote ref.
/// A colon-less spec pushes to the same-named ref, so it is used verbatim.
fn destination_refs(refspecs: &[String]) -> String {
    if refspecs.is_empty() {
        return "the remote".to_string();
    }
    refspecs
        .iter()
        .map(|spec| {
            spec.strip_prefix('+')
                .unwrap_or(spec)
                .split_once(':')
                .map_or(spec.as_str(), |(_, dst)| dst)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Redact credentials embedded in any `scheme://user:secret@host` URL within a
/// remote error message, replacing the userinfo with `***`.
///
/// Remote error messages are user-facing (a rejected ref, an auth failure), but
/// a remote URL can embed a token; only the userinfo is stripped so the rest of
/// the diagnostic stays intact.
pub(crate) fn redact_url_credentials(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    let mut rest = message;
    while let Some(scheme_idx) = rest.find("://") {
        let after = scheme_idx + "://".len();
        out.push_str(&rest[..after]);
        let tail = &rest[after..];
        let authority_end = tail
            .find(|c: char| c == '/' || c == '?' || c == '#' || c.is_whitespace())
            .unwrap_or(tail.len());
        let authority = &tail[..authority_end];
        if let Some(at) = authority.rfind('@') {
            out.push_str("***@");
            out.push_str(&authority[at + 1..]);
        } else {
            out.push_str(authority);
        }
        rest = &tail[authority_end..];
    }
    out.push_str(rest);
    out
}

/// Classify a `git2` error raised while resolving the repository's default signature.
///
/// A missing `user.name` / `user.email` surfaces from libgit2 as a `Config`
/// class, `NotFound` code error. That is an actionable configuration problem —
/// not an internal failure — so it maps to [`GitError::IdentityMissing`] rather
/// than being collapsed into the opaque [`GitError::Internal`] variant.
pub(crate) fn map_signature_error(err: git2::Error) -> GitError {
    if err.class() == git2::ErrorClass::Config && err.code() == git2::ErrorCode::NotFound {
        GitError::IdentityMissing {
            key: identity_key_from_message(err.message()),
        }
    } else {
        GitError::Internal(err)
    }
}

/// Extract the missing identity config key from a libgit2 signature error message.
///
/// libgit2 formats the message as `config value 'user.name' was not found`; the
/// quoted key is recovered when present, otherwise both identity keys are named
/// so the guidance stays actionable regardless of message wording.
fn identity_key_from_message(message: &str) -> String {
    message
        .split('\'')
        .nth(1)
        .filter(|key| key.starts_with("user."))
        .unwrap_or("user.name / user.email")
        .to_string()
}
