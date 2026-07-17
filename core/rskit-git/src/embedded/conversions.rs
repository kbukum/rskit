use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::types::{Commit, Oid, Reference, Signature};

pub(crate) fn oid_from_git2(oid: git2::Oid) -> Oid {
    let mut bytes = [0u8; 20];
    bytes.copy_from_slice(oid.as_bytes());
    Oid::from_bytes(bytes)
}

pub(crate) fn signature_from_git2(signature: &git2::Signature<'_>) -> Signature {
    Signature {
        name: signature.name().unwrap_or_default().to_string(),
        email: signature.email().unwrap_or_default().to_string(),
        when: system_time_from_git2(signature.when()),
    }
}

pub(crate) fn commit_from_git2(commit: &git2::Commit<'_>) -> Commit {
    Commit {
        oid: oid_from_git2(commit.id()),
        author: signature_from_git2(&commit.author()),
        committer: signature_from_git2(&commit.committer()),
        message: commit
            .message()
            .unwrap_or_default()
            .trim_end_matches('\n')
            .trim_end_matches('\0')
            .to_string(),
        parents: commit.parent_ids().map(oid_from_git2).collect(),
    }
}
pub(crate) fn reference_from_git2(reference: &git2::Reference<'_>) -> Reference {
    let name = reference.name().unwrap_or("HEAD").to_string();
    let target = reference
        .resolve()
        .ok()
        .and_then(|resolved| resolved.target())
        .or_else(|| reference.target())
        .map(oid_from_git2)
        .unwrap_or_else(|| Oid::from_bytes([0; 20]));
    Reference {
        name,
        target,
        is_branch: reference.is_branch(),
        is_tag: reference.is_tag(),
    }
}

pub(crate) fn system_time_from_git2(time: git2::Time) -> SystemTime {
    let seconds = time.seconds();
    if seconds >= 0 {
        UNIX_EPOCH + Duration::from_secs(seconds as u64)
    } else {
        UNIX_EPOCH - Duration::from_secs(seconds.unsigned_abs())
    }
}
