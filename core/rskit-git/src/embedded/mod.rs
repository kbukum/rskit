//! Embedded libgit2 repository implementation.

pub mod auth;
mod conversions;
mod errors;
mod manage;
mod read;
mod repository;
#[cfg(test)]
mod tests;
mod write;

pub(crate) use conversions::{
    commit_from_git2, oid_from_git2, reference_from_git2, signature_from_git2,
};
pub(crate) use errors::{map_head_error, map_remote_error, map_signature_error};
pub use repository::{Git2Repository, clone, discover, init, init_bare, init_with, open};
