//! Core git repository traits.

use std::path::Path;

use rskit_errors::AppResult;

use crate::types::{Oid, Reference};

/// Core git repository operations.
pub trait Repository {
    /// Returns the absolute path to the repository root.
    fn root(&self) -> &Path;

    /// Returns the reference that HEAD points to.
    fn head(&self) -> AppResult<Reference>;

    /// Resolves a ref name (branch, tag, or SHA prefix) to an OID.
    fn resolve_ref(&self, refname: &str) -> AppResult<Oid>;

    /// Reports whether the working tree has uncommitted changes.
    fn is_dirty(&self) -> AppResult<bool>;
}

/// Raw git command execution escape hatch.
pub trait Executor {
    /// Executes the git CLI with the provided arguments.
    fn exec(&self, args: &[&str]) -> AppResult<Vec<u8>>;
}
