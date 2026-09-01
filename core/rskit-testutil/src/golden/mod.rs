//! Generic golden/snapshot verification: normalization, matcher tiers, and
//! golden-file read/compare/bless.
//!
//! The module is domain-agnostic: callers supply the normalization [`Rule`]s,
//! pick a [`Match`] tier per comparison, and point a [`Golden`] at the expected
//! file. Setting the [`BLESS_ENV`] environment variable regenerates goldens
//! from live output instead of failing on mismatch.

mod diff;
mod file;
mod json;
mod matcher;
mod normalize;

pub use file::{BLESS_ENV, Golden, GoldenMode, GoldenOutcome};
pub use json::CrossKitJsonGolden;
pub use matcher::Match;
pub use normalize::{Normalizer, Rule};
