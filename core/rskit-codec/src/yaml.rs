use rskit_errors::{AppError, AppResult};
use serde_json::Value;

use crate::codec::Codec;

/// Built-in YAML codec.
///
/// Decodes YAML into the canonical [`Value`] tree and encodes a value tree back to YAML.
/// Like the TOML codec, the top-level value must be a mapping (the crate's document
/// contract for config-shaped formats), and a non-mapping top level surfaces as a
/// typed error rather than a panic — on both decode and encode, so round-trips stay symmetric.
///
/// # Security
///
/// Unlike TOML and JSON, YAML supports anchors and aliases, which the parser expands
/// during decode. A small hostile document can therefore reference-expand into a much
/// larger in-memory tree ("billion laughs"). This codec does not itself cap expansion,
/// so callers must decode only size-bounded input — the same trust boundary the other
/// codecs rely on (e.g. `rskit-fs` bounded reads). Do not feed unbounded or untrusted
/// streams straight into [`decode_value`](YamlCodec::decode_value).
#[derive(Debug, Clone, Copy, Default)]
pub struct YamlCodec;

impl Codec for YamlCodec {
    fn name(&self) -> &'static str {
        "yaml"
    }

    fn encode_value(&self, value: &Value) -> AppResult<String> {
        if !value.is_object() {
            return Err(AppError::invalid_input(
                "codec",
                "failed to serialize value as YAML: top level must be a mapping",
            ));
        }
        serde_norway::to_string(value).map_err(|err| {
            AppError::invalid_input("codec", "failed to serialize value as YAML").with_cause(err)
        })
    }

    fn decode_value(&self, contents: &str) -> AppResult<Value> {
        let value = serde_norway::from_str::<Value>(contents).map_err(|err| {
            AppError::invalid_input("codec", "failed to parse YAML").with_cause(err)
        })?;
        if !value.is_object() {
            return Err(AppError::invalid_input(
                "codec",
                "YAML top level must be a mapping",
            ));
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode;
    use serde::Deserialize;

    #[test]
    fn round_trips_table() {
        let codec = YamlCodec;
        let value: Value = serde_json::json!({
            "name": "svc",
            "tags": ["alpha", "beta", "gamma"],
            "nested": { "enabled": true, "retries": 3 }
        });

        let encoded = codec.encode_value(&value).unwrap();
        let decoded = codec.decode_value(&encoded).unwrap();

        assert_eq!(decoded, value);
        assert_eq!(codec.name(), "yaml");
    }

    #[test]
    fn rejects_malformed_input() {
        let err = YamlCodec.decode_value("key: [unclosed").unwrap_err();
        assert!(err.to_string().contains("parse"));
    }

    #[test]
    fn rejects_non_mapping_top_level_on_decode() {
        // Valid YAML documents, but outside the crate's mapping-root contract.
        for doc in ["- a\n- b", "42", ""] {
            let err = YamlCodec.decode_value(doc).unwrap_err();
            assert!(err.to_string().contains("mapping"), "doc: {doc:?}");
        }
    }

    #[test]
    fn rejects_non_mapping_top_level_on_encode() {
        // Mirrors the decode-side contract so encode → decode stays symmetric.
        let err = YamlCodec.encode_value(&Value::Null).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("serialize"));
        // Unlike TOML (whose serializer rejects `null` natively), this rejection
        // is the codec's own top-level contract — pin the reason.
        assert!(message.contains("mapping"), "names the contract: {message}");
    }

    #[test]
    fn decode_honors_deny_unknown_fields() {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Settings {
            #[expect(dead_code, reason = "only the field set is under test")]
            name: String,
        }

        let err = decode::<Settings>(&YamlCodec, "name: svc\nunknown: 1\n").unwrap_err();
        assert!(err.to_string().contains("deserialize"));
    }
}
