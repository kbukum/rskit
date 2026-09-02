//! Lossless serde adapters for [`Duration`], for use with `#[serde(with = ...)]`.
//!
//! Both the required and [`option`] adapters encode a [`Duration`] as the round-trip-safe string
//! produced by [`super::format_duration_exact`] and decode it with
//! [`super::parse_duration`], so a serialized configuration value never drifts
//! from the value it was written with.
//!
//! ```
//! use std::time::Duration;
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Serialize, Deserialize, PartialEq, Debug)]
//! struct Config {
//!     #[serde(with = "rskit_util::time::serde_duration")]
//!     grace: Duration,
//!     #[serde(with = "rskit_util::time::serde_duration::option")]
//!     ttl: Option<Duration>,
//! }
//!
//! let config = Config { grace: Duration::from_secs(3601), ttl: Some(Duration::from_secs(90)) };
//! let json = serde_json::to_string(&config).unwrap();
//! assert_eq!(serde_json::from_str::<Config>(&json).unwrap(), config);
//! ```

use std::time::Duration;

use serde::{Deserialize, Deserializer, Serializer};

use super::{format_duration_exact, parse_duration};

/// Serialize a [`Duration`] as a lossless, human-readable string.
///
/// # Errors
///
/// Propagates any error raised by the serializer.
pub fn serialize<S>(value: &Duration, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&format_duration_exact(*value))
}

/// Deserialize a [`Duration`] from a duration string.
///
/// # Errors
///
/// Returns an error when the string is not a valid duration.
pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    parse_duration(&raw)
        .ok_or_else(|| serde::de::Error::custom(format!("invalid duration string: {raw}")))
}

/// Lossless serde adapter for [`Option<Duration>`].
pub mod option {
    use super::{
        Deserialize, Deserializer, Duration, Serializer, format_duration_exact, parse_duration,
    };

    /// Serialize an [`Option<Duration>`] as a lossless string or `null`.
    ///
    /// # Errors
    ///
    /// Propagates any error raised by the serializer.
    pub fn serialize<S>(value: &Option<Duration>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(duration) => serializer.serialize_str(&format_duration_exact(*duration)),
            None => serializer.serialize_none(),
        }
    }

    /// Deserialize an [`Option<Duration>`] from a duration string or `null`.
    ///
    /// # Errors
    ///
    /// Returns an error when a present value is not a valid duration string.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = Option::<String>::deserialize(deserializer)?;
        raw.map_or_else(
            || Ok(None),
            |text| {
                parse_duration(&text).map(Some).ok_or_else(|| {
                    serde::de::Error::custom(format!("invalid duration string: {text}"))
                })
            },
        )
    }
}
