use std::fmt;

use rskit_errors::{AppError, AppResult};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// Object-safe contract for a structured-text codec over the [`Value`] model.
///
/// Implementations encode and decode one on-disk/text representation (TOML,
/// JSON, …) to and from [`serde_json::Value`]. The trait is intentionally
/// object-safe so a codec can be held as `Arc<dyn Codec>` and selected at
/// runtime; type-driven conversions live in the free functions [`encode`] and
/// [`decode`].
pub trait Codec: fmt::Debug + Send + Sync + 'static {
    /// A short identifier for diagnostics (for example `"toml"`).
    fn name(&self) -> &'static str;

    /// Encode a value tree into this codec's textual representation.
    ///
    /// # Errors
    ///
    /// Returns a typed [`AppError`] (cause preserved) when `value` cannot be
    /// represented in this format.
    fn encode_value(&self, value: &Value) -> AppResult<String>;

    /// Decode text into a value tree.
    ///
    /// # Errors
    ///
    /// Returns a typed [`AppError`] (cause preserved) when `contents` is
    /// malformed for this format.
    fn decode_value(&self, contents: &str) -> AppResult<Value>;
}

/// Encode any [`Serialize`] value using `codec`.
///
/// The value is first converted to the canonical [`Value`] tree, then encoded.
///
/// # Errors
///
/// Returns a typed [`AppError`] (cause preserved) when the value cannot be
/// converted to the value model or encoded by `codec`.
pub fn encode<T>(codec: &dyn Codec, value: &T) -> AppResult<String>
where
    T: Serialize + ?Sized,
{
    let value = serde_json::to_value(value).map_err(|err| {
        AppError::invalid_input("codec", "failed to convert value into the value model")
            .with_cause(err)
    })?;
    codec.encode_value(&value)
}

/// Decode text into any [`DeserializeOwned`] type using `codec`.
///
/// The text is decoded to the canonical [`Value`] tree, then deserialized into
/// `T`. This honors `#[serde(deny_unknown_fields)]` on `T`.
///
/// # Errors
///
/// Returns a typed [`AppError`] (cause preserved) when `contents` is malformed
/// or does not match `T` (including unknown fields under
/// `deny_unknown_fields`).
pub fn decode<T>(codec: &dyn Codec, contents: &str) -> AppResult<T>
where
    T: DeserializeOwned,
{
    let value = codec.decode_value(contents)?;
    T::deserialize(value).map_err(|err| {
        AppError::invalid_input("codec", "failed to deserialize value").with_cause(err)
    })
}
