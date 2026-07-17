//! Format-agnostic structured dataset record.

use std::collections::BTreeMap;

use serde_json::Value;

/// Format identifier for built-in dataset record readers and writers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DatasetFormat {
    /// Comma-separated values with a header row.
    Csv,
    /// JSON array of records, intended for bounded fixture-style datasets.
    JsonArray,
    /// Newline-delimited JSON records.
    JsonLines,
}

/// Format-agnostic structured dataset row.
#[derive(Debug, Clone, PartialEq)]
pub struct DatasetRecord {
    fields: BTreeMap<String, Value>,
}

impl DatasetRecord {
    /// Create a record from named fields.
    #[must_use]
    pub fn new(fields: BTreeMap<String, Value>) -> Self {
        Self { fields }
    }

    /// Create a record from any iterator of named fields.
    #[must_use]
    pub fn from_fields<I, K>(fields: I) -> Self
    where
        I: IntoIterator<Item = (K, Value)>,
        K: Into<String>,
    {
        Self {
            fields: fields
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect(),
        }
    }

    /// Borrow a field by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.fields.get(name)
    }

    /// Borrow all record fields in deterministic key order.
    #[must_use]
    pub fn fields(&self) -> &BTreeMap<String, Value> {
        &self.fields
    }

    /// Consume this record into its fields.
    #[must_use]
    pub fn into_fields(self) -> BTreeMap<String, Value> {
        self.fields
    }

    /// Return a projected record with only the requested columns.
    #[must_use]
    pub fn select(&self, columns: &[String]) -> Self {
        let fields = columns
            .iter()
            .filter_map(|column| {
                self.fields
                    .get(column)
                    .map(|value| (column.clone(), value.clone()))
            })
            .collect();
        Self { fields }
    }

    /// Convert this record to a JSON object.
    #[must_use]
    pub fn into_json(self) -> Value {
        Value::Object(self.fields.into_iter().collect())
    }

    /// Borrow this record as a JSON object without consuming it.
    #[must_use]
    pub fn to_json(&self) -> Value {
        Value::Object(self.fields.clone().into_iter().collect())
    }
}

impl crate::DatasetItem for DatasetRecord {}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn dataset_record_projection_and_json_are_deterministic() {
        let record = DatasetRecord::new(
            DatasetRecord::from_fields([("b", json!(2)), ("a", json!("one")), ("c", Value::Null)])
                .into_fields(),
        );

        assert_eq!(record.get("a"), Some(&json!("one")));
        assert_eq!(
            record.fields().keys().cloned().collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
        let selected = record.select(&["c".to_string(), "missing".to_string(), "a".to_string()]);
        assert_eq!(
            selected.fields().keys().cloned().collect::<Vec<_>>(),
            ["a", "c"]
        );
        assert_eq!(selected.into_json(), json!({"a":"one","c":null}));
    }
}
