//! Filter data model: filters, targets, and typed parameters.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A named filter operation with typed parameters.
///
/// # Examples
///
/// ```rust
/// use rskit_media::filter::{self, filters};
///
/// let denoise = filters::denoise(3);
/// let sharpen = filters::sharpen(1.5);
/// let grayscale = filters::grayscale();
/// let custom = filters::custom_video("chromakey=0x00FF00:0.1:0.2");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filter {
    /// Filter name (maps to backend filter name).
    pub name: String,
    /// Whether this filter targets video or audio.
    pub target: FilterTarget,
    /// Filter parameters.
    pub params: Params,
}

/// Which stream type a filter targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterTarget {
    /// Video stream filter.
    Video,
    /// Audio stream filter.
    Audio,
}

/// Type-safe parameter map for filters.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Params(HashMap<String, ParamValue>);

impl Params {
    /// Create an empty parameter map.
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    /// Set a parameter value (builder pattern).
    #[must_use]
    pub fn set(mut self, key: impl Into<String>, val: impl Into<ParamValue>) -> Self {
        self.0.insert(key.into(), val.into());
        self
    }

    /// Get a parameter value.
    pub fn get(&self, key: &str) -> Option<&ParamValue> {
        self.0.get(key)
    }

    /// Iterate over parameters.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &ParamValue)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v))
    }
}

/// A filter parameter value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParamValue {
    /// Integer value.
    Int(i64),
    /// Floating-point value.
    Float(f64),
    /// String value.
    Str(String),
    /// Boolean value.
    Bool(bool),
}

impl From<i64> for ParamValue {
    fn from(v: i64) -> Self {
        Self::Int(v)
    }
}

impl From<f64> for ParamValue {
    fn from(v: f64) -> Self {
        Self::Float(v)
    }
}

impl From<String> for ParamValue {
    fn from(v: String) -> Self {
        Self::Str(v)
    }
}

impl From<&str> for ParamValue {
    fn from(v: &str) -> Self {
        Self::Str(v.to_string())
    }
}

impl From<bool> for ParamValue {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}
