//! Pluggable per-item validation seam for the collection engine.
//!
//! The engine treats validation as an injected policy:
//! a [`Validator`] inspects each item after its transforms and rejects it with a typed error.
//! The engine ships no built-in validator —
//! callers opt in (for example a schema-backed validator for tabular records).

use rskit_errors::AppResult;

use crate::DatasetItem;

/// Rejects invalid items before they are materialized by a sink.
pub trait Validator<T: DatasetItem>: Send + Sync {
    /// Return an error when `item` fails validation; `Ok(())` accepts it.
    fn validate(&self, item: &T) -> AppResult<()>;
}
