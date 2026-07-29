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
        GitError::Network(err.message().to_string())
    } else {
        GitError::Internal(err)
    }
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
