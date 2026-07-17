//! Identity-aware include-merge for strict, layered config documents.

mod identity;
mod include_merge;

#[cfg(test)]
mod tests;

pub use identity::{CompositeKey, IdentityKey, MergeIdentity};
pub use include_merge::IncludeMerge;
