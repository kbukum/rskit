use rskit_errors::{AppError, AppResult};
use serde::de::DeserializeOwned;

/// Deserialize merged config into `T`, then apply programmatic defaults.
///
/// This is the validation-free path: any `serde` type loads without depending
/// on `rskit-validation`. Use [`decode_validated`] when the type opts into
/// [`rskit_validation::Validate`].
pub(crate) fn decode_typed<T>(
    raw: config::Config,
    apply_defaults: impl FnOnce(&mut T),
) -> AppResult<T>
where
    T: DeserializeOwned,
{
    let mut cfg: T = raw
        .try_deserialize()
        .map_err(|e| AppError::invalid_input("config", e.to_string()))?;

    apply_defaults(&mut cfg);
    Ok(cfg)
}

/// Deserialize merged config into `T`, apply defaults, then run `Validate`.
#[cfg(feature = "validate")]
pub(crate) fn decode_validated<T>(
    raw: config::Config,
    apply_defaults: impl FnOnce(&mut T),
) -> AppResult<T>
where
    T: DeserializeOwned + rskit_validation::Validate,
{
    let cfg = decode_typed(raw, apply_defaults)?;
    cfg.validate()
        .map_err(|e| AppError::invalid_input("config", e.to_string()))?;
    Ok(cfg)
}
