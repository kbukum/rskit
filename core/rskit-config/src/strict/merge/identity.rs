//! Identity rules for array-of-tables sections during include-merge.
//!
//! A [`MergeIdentity`] extracts a stable identity token from one array element so
//! the merge layer can concatenate registered sections across documents and reject
//! duplicate identities. [`IdentityKey`] covers the common "single named field"
//! case; [`CompositeKey`] covers multi-field and nested identities (for example a
//! cross-reference keyed by `{ from.ecosystem, from.module, to.ecosystem, to.module }`).

use serde_json::Value;

/// Identity rule for an array-of-tables section during include-merge.
///
/// Sections registered with an identity are concatenated across documents rather
/// than overwritten, and duplicate identities are a hard error. Implement this
/// trait for custom identity logic, or use [`IdentityKey`] / [`CompositeKey`].
pub trait MergeIdentity: Send + Sync {
    /// Human-facing label for the identity, used in duplicate-error messages.
    fn label(&self) -> &str;

    /// Extract the identity token of `element`, or `None` when the element lacks
    /// a complete identity (a missing/non-scalar field is a schema error surfaced
    /// by the typed deserialize step, so the merge layer simply skips it here).
    fn identity_of(&self, element: &Value) -> Option<String>;
}

/// A simple [`MergeIdentity`] naming a single identity field directly.
#[derive(Debug, Clone)]
pub struct IdentityKey(String);

impl IdentityKey {
    /// Identify array elements by the value of `field`.
    pub fn new(field: impl Into<String>) -> Self {
        Self(field.into())
    }
}

impl MergeIdentity for IdentityKey {
    fn label(&self) -> &str {
        &self.0
    }

    fn identity_of(&self, element: &Value) -> Option<String> {
        scalar_token(element.get(&self.0)?)
    }
}

/// A [`MergeIdentity`] over several fields, each addressed by a dotted path.
///
/// Use this when no single field identifies an element — for example an edge
/// identified by both of its structured endpoints. Each field is a `.`-separated
/// path walked through nested objects (`"from.ecosystem"` reads
/// `element["from"]["ecosystem"]`); the resulting scalar tokens are combined
/// into one injective composite identity. An element missing any field has no
/// identity and is skipped (the typed schema step reports it); likewise a key
/// built from an empty field list has no identity and never reports duplicates.
#[derive(Debug, Clone)]
pub struct CompositeKey {
    fields: Vec<Vec<String>>,
    label: String,
}

impl CompositeKey {
    /// Identify array elements by the joined values of `fields`.
    ///
    /// Each entry is a dotted path into the element (`"to.module"`).
    pub fn new<I, S>(fields: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let raw: Vec<String> = fields.into_iter().map(Into::into).collect();
        let label = raw.join("+");
        let fields = raw
            .iter()
            .map(|field| field.split('.').map(ToOwned::to_owned).collect())
            .collect();
        Self { fields, label }
    }
}

impl MergeIdentity for CompositeKey {
    fn label(&self) -> &str {
        &self.label
    }

    fn identity_of(&self, element: &Value) -> Option<String> {
        // An empty field list cannot identify anything: returning an empty token
        // here would make every element collide as a "duplicate". Treat it as no
        // identity (consistent with a missing field) so such a section simply
        // concatenates instead of spuriously failing the merge.
        if self.fields.is_empty() {
            return None;
        }
        let mut token = String::new();
        for path in &self.fields {
            let part = scalar_token(lookup(element, path)?)?;
            // Length-prefix each part so the encoding is injective: the byte
            // length unambiguously delimits the part, so no scalar value can
            // forge a field boundary regardless of the characters it contains.
            // A raw separator join would let e.g. `["a\u{1f}b", "c"]` collide
            // with `["a", "b\u{1f}c"]` and reject a valid config as a duplicate.
            token.push_str(part.len().to_string().as_str());
            token.push(':');
            token.push_str(&part);
        }
        Some(token)
    }
}

/// Walk a dotted-path of object keys, returning the addressed value.
fn lookup<'a>(value: &'a Value, path: &[String]) -> Option<&'a Value> {
    let mut current = value;
    for segment in path {
        current = current.get(segment)?;
    }
    Some(current)
}

/// Render a JSON scalar as a stable identity token; non-scalars yield `None`.
fn scalar_token(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn identity_key_reads_named_field() {
        let key = IdentityKey::new("name");
        assert_eq!(
            key.identity_of(&json!({ "name": "a" })),
            Some("a".to_string())
        );
        assert_eq!(key.identity_of(&json!({ "other": "a" })), None);
        assert_eq!(key.label(), "name");
    }

    #[test]
    fn composite_key_joins_nested_fields() {
        let key = CompositeKey::new(["from.ecosystem", "from.module", "to.ecosystem", "to.module"]);
        let element = json!({
            "from": { "ecosystem": "go", "module": "api" },
            "to": { "ecosystem": "rust", "module": "shared" },
        });

        let token = key.identity_of(&element).expect("complete identity");
        assert!(token.contains("go"));
        assert!(token.contains("rust"));
        assert_eq!(
            key.label(),
            "from.ecosystem+from.module+to.ecosystem+to.module"
        );
    }

    #[test]
    fn composite_key_is_none_when_a_field_is_missing() {
        let key = CompositeKey::new(["from.ecosystem", "to.ecosystem"]);
        let element = json!({ "from": { "ecosystem": "go" } });

        assert_eq!(key.identity_of(&element), None);
    }

    #[test]
    fn composite_key_with_no_fields_has_no_identity() {
        // An empty field list must not collapse every element to the same empty
        // token, which would make any multi-element section look duplicated.
        let key = CompositeKey::new(Vec::<String>::new());

        assert_eq!(key.identity_of(&json!({ "name": "a" })), None);
        assert_eq!(key.identity_of(&json!({ "name": "b" })), None);
    }

    #[test]
    fn composite_key_separates_distinct_tuples() {
        let key = CompositeKey::new(["a", "b"]);
        let first = key.identity_of(&json!({ "a": "x", "b": "yz" }));
        let second = key.identity_of(&json!({ "a": "xy", "b": "z" }));

        assert!(first.is_some());
        assert_ne!(first, second);
    }

    #[test]
    fn composite_key_is_injective_for_separator_like_values() {
        let key = CompositeKey::new(["a", "b"]);

        // A raw separator join (U+001F) would let a value containing the
        // separator forge a field boundary; the length-prefixed encoding must
        // keep these distinct tuples distinct.
        let with_sep = key.identity_of(&json!({ "a": "x\u{1f}y", "b": "z" }));
        let split = key.identity_of(&json!({ "a": "x", "b": "y\u{1f}z" }));
        assert!(with_sep.is_some());
        assert_ne!(with_sep, split);

        // The same guarantee must hold for the length-prefix delimiter itself,
        // so a value that looks like an encoded part cannot collide.
        let looks_encoded = key.identity_of(&json!({ "a": "1:x", "b": "y" }));
        let genuinely_split = key.identity_of(&json!({ "a": "1", "b": "x1:y" }));
        assert_ne!(looks_encoded, genuinely_split);
    }
}
