//! Exponential, constant, and linear back-off retry policies.

mod backoff;
mod error;
mod policy;
mod preset;

pub use backoff::{BackoffKind, ConstantBackoff, LinearBackoff};
pub use error::RetryError;
pub use policy::RetryPolicy;
pub use preset::RetryPreset;

#[cfg(test)]
mod tests;
