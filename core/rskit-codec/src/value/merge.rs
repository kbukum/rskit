use serde_json::Value;

/// How to combine two arrays found at the same key during a merge.
///
/// The strategy is chosen per key by the caller (see [`merge_with`]);
/// the merge mechanism itself is policy-free — it does not know what any key "means".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ArrayStrategy {
    /// The overlay array replaces the base array wholesale (last-wins).
    Replace,
    /// The overlay array is appended to the base array (concatenation).
    Concat,
}

/// Deep-merge `overlay` onto `base`, replacing arrays wholesale.
///
/// Objects merge recursively; on a key collision the overlay value wins (last-wins scalars).
/// Every array is replaced by the overlay. Use [`merge_with`] to concatenate selected arrays instead.
#[must_use]
pub fn merge(base: Value, overlay: Value) -> Value {
    merge_with(base, overlay, |_| ArrayStrategy::Replace)
}

/// Deep-merge `overlay` onto `base`, choosing an array strategy per key.
///
/// Objects merge recursively; on a key collision the overlay value wins.
/// When both sides hold an array at the same key,
/// `array_strategy` is consulted with that key to decide [`ArrayStrategy::Replace`] vs [`ArrayStrategy::Concat`].
/// Type mismatches (for example object vs scalar) resolve to the overlay.
///
/// The mechanism is framework-agnostic: identity rules, duplicate detection,
/// and "which keys are mergeable lists" are caller policy expressed through `array_strategy`,
/// not built in here.
#[must_use]
pub fn merge_with<F>(base: Value, overlay: Value, mut array_strategy: F) -> Value
where
    F: FnMut(&str) -> ArrayStrategy,
{
    merge_inner(base, overlay, None, &mut array_strategy)
}

fn merge_inner<F>(base: Value, overlay: Value, key: Option<&str>, array_strategy: &mut F) -> Value
where
    F: FnMut(&str) -> ArrayStrategy,
{
    match (base, overlay) {
        (Value::Object(mut base), Value::Object(overlay)) => {
            for (key, overlay_value) in overlay {
                let merged = match base.remove(&key) {
                    Some(base_value) => {
                        merge_inner(base_value, overlay_value, Some(&key), array_strategy)
                    }
                    None => overlay_value,
                };
                base.insert(key, merged);
            }
            Value::Object(base)
        }
        (Value::Array(mut base), Value::Array(overlay)) => {
            match key.map_or(ArrayStrategy::Replace, array_strategy) {
                ArrayStrategy::Concat => {
                    base.extend(overlay);
                    Value::Array(base)
                }
                ArrayStrategy::Replace => Value::Array(overlay),
            }
        }
        // Scalars and type mismatches: overlay wins.
        (_, overlay) => overlay,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn overlay_scalar_wins_last() {
        let merged = merge(
            json!({ "name": "base", "retries": 1 }),
            json!({ "retries": 5 }),
        );

        assert_eq!(merged, json!({ "name": "base", "retries": 5 }));
    }

    #[test]
    fn objects_merge_recursively() {
        let merged = merge(
            json!({ "server": { "host": "a", "port": 1 } }),
            json!({ "server": { "port": 2 } }),
        );

        assert_eq!(merged, json!({ "server": { "host": "a", "port": 2 } }));
    }

    #[test]
    fn overlay_adds_new_keys() {
        let merged = merge(json!({ "a": 1 }), json!({ "b": 2 }));

        assert_eq!(merged, json!({ "a": 1, "b": 2 }));
    }

    #[test]
    fn unselected_arrays_replace_under_concat_strategy() {
        let merged = merge_with(
            json!({ "groups": [1], "ports": [1, 2] }),
            json!({ "groups": [2], "ports": [9] }),
            |key| {
                if key == "groups" {
                    ArrayStrategy::Concat
                } else {
                    ArrayStrategy::Replace
                }
            },
        );

        assert_eq!(merged, json!({ "groups": [1, 2], "ports": [9] }));
    }

    #[test]
    fn arrays_replace_by_default() {
        let merged = merge(json!({ "ports": [1, 2, 3] }), json!({ "ports": [9] }));

        assert_eq!(merged, json!({ "ports": [9] }));
    }

    #[test]
    fn selected_arrays_concatenate() {
        let merged = merge_with(
            json!({ "groups": [{ "name": "a" }] }),
            json!({ "groups": [{ "name": "b" }] }),
            |key| {
                if key == "groups" {
                    ArrayStrategy::Concat
                } else {
                    ArrayStrategy::Replace
                }
            },
        );

        assert_eq!(
            merged,
            json!({ "groups": [{ "name": "a" }, { "name": "b" }] })
        );
    }

    #[test]
    fn type_mismatch_resolves_to_overlay() {
        let merged = merge(json!({ "x": { "deep": true } }), json!({ "x": 5 }));

        assert_eq!(merged, json!({ "x": 5 }));
    }
}
