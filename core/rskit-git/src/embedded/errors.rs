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
