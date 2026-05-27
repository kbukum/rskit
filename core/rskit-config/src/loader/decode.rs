use rskit_errors::{AppError, AppResult};
use rskit_validation::Validate;
use serde::de::DeserializeOwned;

pub(crate) fn decode<T>(raw: config::Config, apply_defaults: impl FnOnce(&mut T)) -> AppResult<T>
where
    T: DeserializeOwned + Validate,
{
    let mut cfg: T = raw
        .try_deserialize()
        .map_err(|e| AppError::invalid_input("config", e.to_string()))?;

    apply_defaults(&mut cfg);
    cfg.validate()
        .map_err(|e| AppError::invalid_input("config", e.to_string()))?;

    Ok(cfg)
}
