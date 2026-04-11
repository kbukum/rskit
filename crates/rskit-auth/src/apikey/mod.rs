//! API key generation, hashing, validation, rotation with grace periods.

mod key;
mod middleware;
mod rotation;
mod store;

pub use key::{generate, hash_key, validate, GenerateResult, Key, KeyValidationError};
pub use middleware::{ApiKeyLayer, KeyValidator};
pub use rotation::{rotate, RotationConfig, RotationResult, DEFAULT_GRACE_PERIOD};
pub use store::Store;
