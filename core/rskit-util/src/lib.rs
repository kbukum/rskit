//! Pure utility functions for rskit.
//!
//! Provides sanitisation, parsing, clock abstractions, and deep-merge
//! helpers that mirror the corresponding `gokit/util` and `pykit-util`
//! packages.
//!
//! # Modules
//!
//! | Module | Description |
//! |----------|----------------------------------------------|
//! | [`sanitize`] | String sanitisation and basic safety checks |
//! | [`parse`] | Human-readable size parsing and secret masking |
//! | [`clock`] | Deterministic clock trait for testable code |
//! | [`merge`] | Deep-merge for [`serde_json::Value`] maps |

#![warn(missing_docs)]

/// String sanitisation and basic injection-pattern detection.
pub mod sanitize;

/// Human-readable size parsing and secret masking.
pub mod parse;

/// Deterministic clock abstraction for testable time-dependent code.
pub mod clock;

/// Deep-merge utilities for [`serde_json::Value`].
pub mod merge;

/// Secret-value wrappers that mask content in Display, Debug, and serialisation.
pub mod secret;

pub use clock::{Clock, FakeClock, SystemClock};
pub use merge::deep_merge;
pub use parse::{mask_secret, parse_size};
pub use sanitize::{is_safe_string, sanitize_env_value, sanitize_string};
pub use secret::SecretString;
