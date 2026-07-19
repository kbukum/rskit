use rskit_errors::{AppError, AppResult};
use serde_json::Value;

use crate::codec::Codec;

/// JSON output layout for [`JsonCodec`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum JsonStyle {
    /// Human-readable, indented output.
    #[default]
    Pretty,
    /// Minimal single-line output for machine streams (newline-delimited JSON, length-framed payloads) where size
    /// and one-value-per-line matter.
    Compact,
}

/// Built-in JSON codec.
///
/// Always available because [`serde_json`] backs the crate's value model.
/// Defaults to pretty-printed output;
/// [`JsonCodec::compact`] emits minimal single-line JSON for machine-readable streams.
#[derive(Debug, Clone, Copy, Default)]
pub struct JsonCodec {
    style: JsonStyle,
}

impl JsonCodec {
    /// A pretty-printing JSON codec.
    #[must_use]
    pub const fn pretty() -> Self {
        Self {
            style: JsonStyle::Pretty,
        }
    }

    /// A compact (single-line) JSON codec for machine streams.
    #[must_use]
    pub const fn compact() -> Self {
        Self {
            style: JsonStyle::Compact,
        }
    }
}

impl Codec for JsonCodec {
    fn name(&self) -> &'static str {
        "json"
    }

    fn encode_value(&self, value: &Value) -> AppResult<String> {
        let result = match self.style {
            JsonStyle::Pretty => serde_json::to_string_pretty(value),
            JsonStyle::Compact => serde_json::to_string(value),
        };
        result.map_err(|err| {
            AppError::invalid_input("codec", "failed to serialize value as JSON").with_cause(err)
        })
    }

    fn decode_value(&self, contents: &str) -> AppResult<Value> {
        serde_json::from_str(contents)
            .map_err(|err| AppError::invalid_input("codec", "failed to parse JSON").with_cause(err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_value() {
        let codec = JsonCodec::default();
        let value: Value = serde_json::json!({ "a": 1, "nested": { "b": true } });

        let encoded = codec.encode_value(&value).unwrap();
        let decoded = codec.decode_value(&encoded).unwrap();

        assert_eq!(decoded, value);
        assert_eq!(codec.name(), "json");
    }

    #[test]
    fn pretty_is_multiline_compact_is_single_line() {
        let value: Value = serde_json::json!({ "a": 1, "b": 2 });
        assert!(
            JsonCodec::pretty()
                .encode_value(&value)
                .unwrap()
                .contains('\n')
        );
        assert!(
            !JsonCodec::compact()
                .encode_value(&value)
                .unwrap()
                .contains('\n')
        );
    }

    #[test]
    fn rejects_malformed_input() {
        let err = JsonCodec::default().decode_value("{ not json").unwrap_err();
        assert!(err.to_string().contains("parse"));
    }
}
