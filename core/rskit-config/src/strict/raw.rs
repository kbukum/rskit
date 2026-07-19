use std::collections::BTreeMap;

use rskit_errors::{AppError, AppResult};
use serde::de::DeserializeOwned;

/// A raw value retained verbatim for later, downstream parsing.
///
/// This is the canonical [`serde_json::Value`] tree shared across rskit (see `rskit-codec`),
/// regardless of the on-disk format the document was parsed from.
pub type RawValue = serde_json::Value;

/// A map of dynamic-keyed raw subtrees (for example `[ecosystems.<id>]`).
///
/// Each subtree is kept as an un-deserialized [`RawValue`] so a downstream owner — a provider, plugin,
/// or adapter — can parse it later with its own `#[serde(deny_unknown_fields)]` schema.
/// The strict top-level document rejects unknown *reserved* keys while a [`RawTable`] field captures the open,
/// dynamic-keyed sections verbatim.
pub type RawTable = BTreeMap<String, RawValue>;

/// Deserialize a retained raw subtree into a concrete type.
///
/// Honors `#[serde(deny_unknown_fields)]` on `T`. `field` names the subtree for error reporting
/// and is included in the returned [`AppError`] on failure.
pub fn deserialize_subtree<T>(field: &str, value: RawValue) -> AppResult<T>
where
    T: DeserializeOwned,
{
    T::deserialize(value).map_err(|error| {
        AppError::invalid_input(field, format!("invalid '{field}' configuration: {error}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    #[serde(deny_unknown_fields)]
    struct Sub {
        enabled: bool,
        retries: u16,
    }

    #[test]
    fn deserialize_subtree_parses_typed_value() {
        let value: RawValue = serde_json::json!({ "enabled": true, "retries": 3 });

        let sub: Sub = deserialize_subtree("driver", value).unwrap();

        assert_eq!(
            sub,
            Sub {
                enabled: true,
                retries: 3
            }
        );
    }

    #[test]
    fn deserialize_subtree_rejects_unknown_field_with_named_error() {
        let value: RawValue = serde_json::json!({ "enabled": true, "retries": 3, "extra": 1 });

        let err = deserialize_subtree::<Sub>("driver", value).unwrap_err();

        assert!(err.to_string().contains("driver"));
        assert!(err.to_string().contains("unknown field"));
    }
}
