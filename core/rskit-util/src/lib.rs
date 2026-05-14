//! Minimal domain-free utility crate for rskit.
//!
//! Foundation crates must stay cheap to depend on. Domain-owned helpers such as
//! secret masking, validation, schema handling, and config merging live in their
//! owning crates instead of this L0 crate.

#![warn(missing_docs)]
