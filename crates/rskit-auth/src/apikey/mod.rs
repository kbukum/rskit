//! API key generation, hashing, validation, rotation with grace periods.

mod key;
mod middleware;
mod rotation;
mod store;

pub use key::{GenerateResult, Key, KeyValidationError, generate, hash_key, validate};
pub use middleware::{ApiKeyLayer, KeyValidator};
pub use rotation::{DEFAULT_GRACE_PERIOD, RotationConfig, RotationResult, rotate};
pub use store::Store;
