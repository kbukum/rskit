//! Deep-merge utilities for [`serde_json::Value`] maps.

use serde_json::Value;

/// Recursively merge `override_val` into `base` (override wins).
///
/// Returns a **new** [`Value`] — neither input is mutated.
/// When both values for a key are objects they are merged recursively;
/// otherwise the override value replaces the base.
pub fn deep_merge(base: &Value, override_val: &Value) -> Value {
    match (base, override_val) {
        (Value::Object(base_map), Value::Object(over_map)) => {
            let mut result = base_map.clone();
            for (key, over_v) in over_map {
                let merged = result
                    .get(key)
                    .map_or_else(|| over_v.clone(), |base_v| deep_merge(base_v, over_v));
                result.insert(key.clone(), merged);
            }
            Value::Object(result)
        }
        (_, override_val) => override_val.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn shallow_merge() {
        let base = json!({"a": 1, "b": 2});
        let over = json!({"b": 3, "c": 4});
        let result = deep_merge(&base, &over);
        assert_eq!(result, json!({"a": 1, "b": 3, "c": 4}));
    }

    #[test]
    fn nested_merge() {
        let base = json!({"db": {"host": "localhost", "port": 5432}});
        let over = json!({"db": {"port": 3306, "name": "mydb"}});
        let result = deep_merge(&base, &over);
        assert_eq!(
            result,
            json!({"db": {"host": "localhost", "port": 3306, "name": "mydb"}})
        );
    }

    #[test]
    fn override_replaces_non_object() {
        let base = json!({"a": 1});
        let over = json!({"a": [1, 2, 3]});
        let result = deep_merge(&base, &over);
        assert_eq!(result, json!({"a": [1, 2, 3]}));
    }

    #[test]
    fn empty_base() {
        let base = json!({});
        let over = json!({"a": 1});
        assert_eq!(deep_merge(&base, &over), json!({"a": 1}));
    }

    #[test]
    fn empty_override() {
        let base = json!({"a": 1});
        let over = json!({});
        assert_eq!(deep_merge(&base, &over), json!({"a": 1}));
    }

    #[test]
    fn deeply_nested() {
        let base = json!({"l1": {"l2": {"l3": {"a": 1}}}});
        let over = json!({"l1": {"l2": {"l3": {"b": 2}}}});
        let result = deep_merge(&base, &over);
        assert_eq!(result, json!({"l1": {"l2": {"l3": {"a": 1, "b": 2}}}}));
    }

    #[test]
    fn does_not_mutate_inputs() {
        let base = json!({"a": 1});
        let over = json!({"b": 2});
        let _result = deep_merge(&base, &over);
        assert_eq!(base, json!({"a": 1}));
        assert_eq!(over, json!({"b": 2}));
    }
}
