use std::collections::{BTreeMap, BTreeSet};

use rskit_codec::value::{ArrayStrategy, merge_with};
use rskit_errors::{AppError, AppResult};
use rskit_util::collections::ensure_unique_by;
use serde_json::Value;

use super::MergeIdentity;

/// Identity-aware include-merge configuration.
///
/// Merges config documents with deterministic, schema-aware rules:
///
/// - tables merge recursively; on a scalar key collision the overlay value wins (last-wins scalars);
/// - array-of-tables sections registered via [`IncludeMerge::with_identity`] are concatenated across documents
///   and hard-error on duplicate identity;
/// - map sections registered via [`IncludeMerge::with_unique_keys`] hard-error when the same map key is contributed by more than one document (a duplicate identity for sections keyed by name rather than by an array element);
/// - any other array is replaced wholesale by the overlay.
#[derive(Default)]
pub struct IncludeMerge {
    identity_sections: BTreeMap<String, Box<dyn MergeIdentity>>,
    unique_key_sections: BTreeSet<String>,
}

impl std::fmt::Debug for IncludeMerge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IncludeMerge")
            .field("identity_sections", &self.identity_sections.keys())
            .field("unique_key_sections", &self.unique_key_sections)
            .finish()
    }
}

impl IncludeMerge {
    /// Create an include-merge with no identity-keyed sections.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `section` as an identity-keyed array-of-tables.
    ///
    /// Such sections are concatenated across documents
    /// and hard-error on a duplicate identity (see [`IdentityKey`](super::IdentityKey) / [`CompositeKey`](super::CompositeKey)).
    #[must_use]
    pub fn with_identity(
        mut self,
        section: impl Into<String>,
        identity: impl MergeIdentity + 'static,
    ) -> Self {
        self.identity_sections
            .insert(section.into(), Box::new(identity));
        self
    }

    /// Register `section` as a map whose keys must be unique across documents.
    ///
    /// Use this for sections modelled as a table-of-tables keyed by name (for example `[groups.<name>]`):
    /// a key contributed by two documents is a duplicate identity and a hard error,
    /// rather than a silent recursive merge.
    #[must_use]
    pub fn with_unique_keys(mut self, section: impl Into<String>) -> Self {
        self.unique_key_sections.insert(section.into());
        self
    }

    /// Merge `overlay` onto `base`, returning the combined document.
    ///
    /// Delegates the value-tree mechanics to [`rskit_codec::value::merge_with`]: objects merge recursively,
    /// scalars are last-wins,
    /// and arrays under a registered identity section are concatenated (all others replaced).
    /// Before merging,
    /// any [`with_unique_keys`](Self::with_unique_keys) section is checked for keys present in both documents,
    /// since the recursive merge would otherwise silently collapse the collision.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] when a unique-key section receives the same key from both documents.
    pub fn merge(&self, base: Value, overlay: Value) -> AppResult<Value> {
        if !self.unique_key_sections.is_empty() {
            self.check_unique_keys(&base, &overlay)?;
        }
        let identity_sections = &self.identity_sections;
        Ok(merge_with(base, overlay, |key| {
            if identity_sections.contains_key(key) {
                ArrayStrategy::Concat
            } else {
                ArrayStrategy::Replace
            }
        }))
    }

    /// Validate identity-keyed sections in a (possibly merged) document.
    ///
    /// Detects duplicate identities within every registered array section, covering both single-document
    /// and merged-document cases.
    pub(crate) fn validate(&self, value: &Value) -> AppResult<()> {
        match value {
            Value::Object(table) => {
                for (key, child) in table {
                    if let (Some(identity), Value::Array(elements)) =
                        (self.identity_sections.get(key), child)
                    {
                        check_unique_identities(key, identity.as_ref(), elements)?;
                    }
                    self.validate(child)?;
                }
            }
            // Recurse into array elements
            // so identity-keyed sections nested inside a list (e.g. an object under an array) are still checked.
            Value::Array(elements) => {
                for element in elements {
                    self.validate(element)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Reject keys contributed to a unique-key section by both documents.
    ///
    /// Walks `base` and `overlay` in lockstep: for each registered section that is a table in both,
    /// an overlapping member key is a duplicate identity. Recurses through shared objects
    /// so a nested registered section is still found.
    fn check_unique_keys(&self, base: &Value, overlay: &Value) -> AppResult<()> {
        let (Value::Object(base), Value::Object(overlay)) = (base, overlay) else {
            return Ok(());
        };
        for (key, overlay_child) in overlay {
            let Some(base_child) = base.get(key) else {
                continue;
            };
            if self.unique_key_sections.contains(key)
                && let (Value::Object(base_section), Value::Object(overlay_section)) =
                    (base_child, overlay_child)
            {
                for member in overlay_section.keys() {
                    if base_section.contains_key(member) {
                        return Err(AppError::invalid_input(
                            key,
                            format!(
                                "duplicate '{member}' in section '{key}' across merged documents"
                            ),
                        ));
                    }
                }
            }
            self.check_unique_keys(base_child, overlay_child)?;
        }
        Ok(())
    }
}

fn check_unique_identities(
    section: &str,
    identity: &dyn MergeIdentity,
    elements: &[Value],
) -> AppResult<()> {
    let identities = elements
        .iter()
        .filter_map(|element| identity.identity_of(element));
    ensure_unique_by(identities, Clone::clone).map_err(|duplicate| {
        AppError::invalid_input(
            section,
            format!(
                "duplicate {} identity '{duplicate}' in section '{section}'",
                identity.label()
            ),
        )
    })
}
