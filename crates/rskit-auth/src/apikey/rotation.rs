//! API key rotation with grace periods.

use chrono::{DateTime, Duration, Utc};
use rskit_errors::AppError;

use super::{generate, validate, GenerateResult, Store};

/// Default grace period: 7 days.
pub const DEFAULT_GRACE_PERIOD: Duration = Duration::days(7);

/// Configuration for key rotation.
#[derive(Debug, Clone)]
pub struct RotationConfig {
    /// How long the old key remains valid after rotation.
    pub grace_period: Duration,
    /// Prefix for the newly generated key (e.g. `"sk_live_"`).
    pub prefix: String,
}

impl Default for RotationConfig {
    fn default() -> Self {
        Self {
            grace_period: DEFAULT_GRACE_PERIOD,
            prefix: String::new(),
        }
    }
}

/// Outcome of a key rotation.
#[derive(Debug, Clone)]
pub struct RotationResult {
    /// The newly generated key material.
    pub new_key: GenerateResult,
    /// The ID of the key that was replaced.
    pub old_key_id: String,
    /// When the old key stops being accepted.
    pub grace_ends_at: DateTime<Utc>,
}

/// Generate a replacement key and set a grace period on the old one.
///
/// The old key remains valid until `grace_ends_at`.
pub async fn rotate(
    store: &dyn Store,
    old_key_id: &str,
    cfg: Option<RotationConfig>,
) -> Result<RotationResult, AppError> {
    let cfg = cfg.unwrap_or_default();
    let old_key = store.get_by_id(old_key_id).await?;
    validate(&old_key).map_err(|e| AppError::invalid_input("key", e.to_string()))?;

    let new_result = generate(&cfg.prefix);
    let grace_ends_at = Utc::now() + cfg.grace_period;

    store
        .set_grace_period(old_key_id, grace_ends_at, None)
        .await?;

    Ok(RotationResult {
        new_key: new_result,
        old_key_id: old_key_id.to_string(),
        grace_ends_at,
    })
}
