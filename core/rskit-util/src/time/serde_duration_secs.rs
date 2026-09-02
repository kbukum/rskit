//! Serde adapters for [`Duration`] values expressed in **whole seconds**, for use with
//! `#[serde(with = ...)]`.
//!
//! This is the cross-kit wire vocabulary for coarse timeouts: sibling kits encode these fields as
//! a bare integer count of seconds (for example `request_timeout: 30`), so the adapter decodes an
//! integer as seconds. As an ergonomic superset it also accepts a human-readable duration string
//! (for example `"30s"`, `"500ms"`, `"1.5m"`) parsed by [`super::parse_duration`], letting rskit
//! configuration stay expressive without breaking the shared integer-seconds contract. Values
//! serialize back as an integer number of seconds.
//!
//! Use [`serde_duration`](super::serde_duration) instead when sub-second precision must round-trip
//! losslessly; this adapter is second-granular by contract.
//!
//! ```
//! use std::time::Duration;
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Serialize, Deserialize, PartialEq, Debug)]
//! struct Config {
//!     #[serde(with = "rskit_util::time::serde_duration_secs")]
//!     request_timeout: Duration,
//! }
//!
//! // Integer seconds (cross-kit wire form).
//! let from_int: Config = serde_json::from_str(r#"{"request_timeout":30}"#).unwrap();
//! assert_eq!(from_int.request_timeout, Duration::from_secs(30));
//!
//! // Human-readable string (rskit convenience superset).
//! let from_str: Config = serde_json::from_str(r#"{"request_timeout":"1.5m"}"#).unwrap();
//! assert_eq!(from_str.request_timeout, Duration::from_secs(90));
//! ```

use std::fmt;
use std::time::Duration;

use serde::de::{self, Deserializer, Visitor};
use serde::ser::Serializer;

use super::parse_duration;

/// Serialize a [`Duration`] as an integer number of whole seconds.
///
/// # Errors
///
/// Propagates any error raised by the serializer.
pub fn serialize<S>(value: &Duration, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_u64(value.as_secs())
}

/// Deserialize a [`Duration`] from integer seconds or a human-readable duration string.
///
/// # Errors
///
/// Returns an error for a negative number or an unparseable duration string.
pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_any(DurationSecsVisitor)
}

struct DurationSecsVisitor;

impl Visitor<'_> for DurationSecsVisitor {
    type Value = Duration;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an integer number of seconds or a duration string like \"30s\"")
    }

    fn visit_u64<E>(self, seconds: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Duration::from_secs(seconds))
    }

    fn visit_i64<E>(self, seconds: i64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let Ok(seconds) = u64::try_from(seconds) else {
            return Err(E::custom(format!(
                "duration seconds must not be negative: {seconds}"
            )));
        };
        Ok(Duration::from_secs(seconds))
    }

    fn visit_f64<E>(self, seconds: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Duration::try_from_secs_f64(seconds)
            .map_err(|error| E::custom(format!("invalid duration seconds {seconds}: {error}")))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        parse_duration(value).ok_or_else(|| E::custom(format!("invalid duration string: {value}")))
    }
}

/// Serde adapter for [`Option<Duration>`] expressed in whole seconds.
pub mod option {
    use super::{Deserializer, Duration, DurationSecsVisitor, Serializer};

    /// Serialize an [`Option<Duration>`] as an integer number of seconds, or `null`.
    ///
    /// # Errors
    ///
    /// Propagates any error raised by the serializer.
    pub fn serialize<S>(value: &Option<Duration>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(duration) => serializer.serialize_u64(duration.as_secs()),
            None => serializer.serialize_none(),
        }
    }

    /// Deserialize an [`Option<Duration>`] from integer seconds, a duration string, or `null`.
    ///
    /// # Errors
    ///
    /// Returns an error when a present value is negative or an unparseable duration string.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_option(OptionVisitor)
    }

    struct OptionVisitor;

    impl<'de> serde::de::Visitor<'de> for OptionVisitor {
        type Value = Option<Duration>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("null, an integer number of seconds, or a duration string")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_any(DurationSecsVisitor).map(Some)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Plain {
        #[serde(with = "super")]
        timeout: Duration,
    }

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Optional {
        #[serde(with = "super::option")]
        timeout: Option<Duration>,
    }

    #[test]
    fn decodes_integer_seconds_and_string_forms() {
        assert_eq!(
            serde_json::from_str::<Plain>(r#"{"timeout":30}"#)
                .unwrap()
                .timeout,
            Duration::from_secs(30)
        );
        assert_eq!(
            serde_json::from_str::<Plain>(r#"{"timeout":"45s"}"#)
                .unwrap()
                .timeout,
            Duration::from_secs(45)
        );
        assert_eq!(
            serde_json::from_str::<Plain>(r#"{"timeout":"2m"}"#)
                .unwrap()
                .timeout,
            Duration::from_mins(2)
        );
    }

    #[test]
    fn serializes_as_integer_seconds() {
        let json = serde_json::to_string(&Plain {
            timeout: Duration::from_secs(90),
        })
        .unwrap();
        assert_eq!(json, r#"{"timeout":90}"#);
    }

    #[test]
    fn rejects_negative_and_invalid_strings() {
        assert!(serde_json::from_str::<Plain>(r#"{"timeout":-1}"#).is_err());
        assert!(serde_json::from_str::<Plain>(r#"{"timeout":"nope"}"#).is_err());
    }

    #[test]
    fn optional_handles_null_int_and_string() {
        assert_eq!(
            serde_json::from_str::<Optional>(r#"{"timeout":null}"#)
                .unwrap()
                .timeout,
            None
        );
        assert_eq!(
            serde_json::from_str::<Optional>(r#"{"timeout":15}"#)
                .unwrap()
                .timeout,
            Some(Duration::from_secs(15))
        );
        assert_eq!(
            serde_json::from_str::<Optional>(r#"{"timeout":"250ms"}"#)
                .unwrap()
                .timeout,
            Some(Duration::from_millis(250))
        );
    }
}
