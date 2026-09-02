//! Request-body assembly helpers shared across provider dialects.

use serde_json::{Map, Value};

/// Merge opaque caller-supplied extension fields into a serialized request body.
///
/// Entries in `extra` are written at the top level of `body` as a provider
/// escape hatch for wire fields the typed request does not model. Keys already
/// produced by the typed wire struct are left untouched, so `extra` can only add
/// fields, never override validated ones. Does nothing when `extra` is empty or
/// `body` is not a JSON object.
pub fn merge_extra(body: &mut Value, extra: &Map<String, Value>) {
    if extra.is_empty() {
        return;
    }
    if let Value::Object(map) = body {
        for (key, value) in extra {
            map.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merge_extra_adds_new_fields() {
        let mut body = json!({"model": "m"});
        let extra = json!({"seed": 7, "logit_bias": {"1": -1}})
            .as_object()
            .unwrap()
            .clone();
        merge_extra(&mut body, &extra);
        assert_eq!(body["seed"], 7);
        assert_eq!(body["logit_bias"]["1"], -1);
        assert_eq!(body["model"], "m");
    }

    #[test]
    fn merge_extra_does_not_override_typed_fields() {
        let mut body = json!({"model": "typed"});
        let extra = json!({"model": "override"}).as_object().unwrap().clone();
        merge_extra(&mut body, &extra);
        assert_eq!(body["model"], "typed");
    }

    #[test]
    fn merge_extra_empty_is_noop() {
        let mut body = json!({"model": "m"});
        merge_extra(&mut body, &Map::new());
        assert_eq!(body, json!({"model": "m"}));
    }

    #[test]
    fn merge_extra_ignores_non_object_body() {
        let mut body = json!("scalar");
        let extra = json!({"k": "v"}).as_object().unwrap().clone();
        merge_extra(&mut body, &extra);
        assert_eq!(body, json!("scalar"));
    }
}
