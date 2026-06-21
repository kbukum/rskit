use rskit_errors::{AppError, AppResult};
use serde_json::Value;

use crate::codec::Codec;

/// Built-in JSON codec.
///
/// Always available because [`serde_json`] backs the crate's value model.
/// Encodes as pretty-printed JSON.
#[derive(Debug, Clone, Copy, Default)]
pub struct JsonCodec;

impl Codec for JsonCodec {
    fn name(&self) -> &'static str {
        "json"
    }

    fn encode_value(&self, value: &Value) -> AppResult<String> {
        serde_json::to_string_pretty(value).map_err(|err| {
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
        let codec = JsonCodec;
        let value: Value = serde_json::json!({ "a": 1, "nested": { "b": true } });

        let encoded = codec.encode_value(&value).unwrap();
        let decoded = codec.decode_value(&encoded).unwrap();

        assert_eq!(decoded, value);
        assert_eq!(codec.name(), "json");
    }

    #[test]
    fn rejects_malformed_input() {
        let err = JsonCodec.decode_value("{ not json").unwrap_err();
        assert!(err.to_string().contains("parse"));
    }
}
