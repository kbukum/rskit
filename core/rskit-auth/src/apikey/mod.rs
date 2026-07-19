//! API key generation, hashing, validation, rotation with grace periods.

mod key;
mod manager;
mod middleware;
mod rotation;
mod store;
#[cfg(test)]
mod test_support;

pub use key::{
    GenerateResult, Hasher, HashingConfig, Key, KeyValidationError, split_key, validate,
};
pub use manager::{KeySpec, Manager};
pub use middleware::{ApiKeyLayer, KeyValidator};
pub use rotation::{DEFAULT_GRACE_PERIOD, RotationConfig, RotationResult};
pub use store::Store;
