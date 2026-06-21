//! Strict, layered config loading for schema-strict consumers.
//!
//! This module is independent of the [`crate::source`] pipeline and the
//! `validate` feature. It targets consumers that need:
//!
//! - hard rejection of unknown keys via `#[serde(deny_unknown_fields)]`
//!   (files are decoded through [`rskit_codec`] into the canonical value tree
//!   and deserialized from it, so serde's unknown-field rejection actually
//!   fires);
//! - verbatim retention of dynamic-keyed sections as [`RawValue`] / [`RawTable`]
//!   for downstream, owner-specific parsing;
//! - identity-aware multi-file include-merge that hard-errors on duplicate
//!   identities.
//!
//! The on-disk format is pluggable per [`StrictLoader`] via [`rskit_codec::Codec`]
//! (TOML by default).

mod document;
mod merge;
mod raw;

pub use document::{StrictLoader, load_strict};
pub use merge::{IdentityKey, IncludeMerge, MergeIdentity};
pub use raw::{RawTable, RawValue, deserialize_subtree};
