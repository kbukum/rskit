//! Selectable options: [`Choice`] and its stable [`ChoiceId`].

use std::fmt;

use serde::{Deserialize, Serialize};

/// The stable identifier a caller uses to recognise a chosen [`Choice`].
///
/// A `ChoiceId` is opaque data (not a closure):
/// the caller maps it back to domain meaning after the prompt returns.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ChoiceId(String);

impl ChoiceId {
    /// Create a choice identifier from any string-like value.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Borrow the identifier as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume the identifier, returning the owned string.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

impl From<&str> for ChoiceId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl From<String> for ChoiceId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl fmt::Display for ChoiceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A single selectable option: pure data carrying an id, a human label, an optional annotation line,
/// and whether it is the recommended default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choice {
    id: ChoiceId,
    label: String,
    annotation: Option<String>,
    recommended: bool,
}

impl Choice {
    /// Create a choice from an id and human-readable label.
    #[must_use]
    pub fn new(id: impl Into<ChoiceId>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            annotation: None,
            recommended: false,
        }
    }

    /// Attach a secondary annotation line (for example `detected in dev-deps`).
    #[must_use]
    pub fn with_annotation(mut self, annotation: impl Into<String>) -> Self {
        self.annotation = Some(annotation.into());
        self
    }

    /// Mark this choice as the recommended default.
    ///
    /// In [`PromptMode::NonInteractive`](crate::prompt::PromptMode::NonInteractive) a `select` resolves to the recommended choice,
    /// and an interactive prompt offers it when the answer is left blank.
    #[must_use]
    pub const fn recommended(mut self) -> Self {
        self.recommended = true;
        self
    }

    /// The stable identifier of this choice.
    #[must_use]
    pub const fn id(&self) -> &ChoiceId {
        &self.id
    }

    /// The human-readable label.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// The optional annotation line, if any.
    #[must_use]
    pub fn annotation(&self) -> Option<&str> {
        self.annotation.as_deref()
    }

    /// Whether this choice is the recommended default.
    #[must_use]
    pub const fn is_recommended(&self) -> bool {
        self.recommended
    }
}

#[cfg(test)]
mod tests {
    use super::{Choice, ChoiceId};

    #[test]
    fn choice_id_borrows_and_consumes_its_string() {
        let id = ChoiceId::new("rust");
        assert_eq!(id.as_str(), "rust");
        assert_eq!(id.into_string(), "rust");
    }

    #[test]
    fn choice_id_converts_from_owned_and_borrowed_strings() {
        assert_eq!(ChoiceId::from("go").as_str(), "go");
        assert_eq!(ChoiceId::from("node".to_string()).as_str(), "node");
    }

    #[test]
    fn choice_id_displays_its_inner_value() {
        assert_eq!(ChoiceId::new("rust").to_string(), "rust");
    }

    #[test]
    fn choice_accessors_expose_metadata() {
        let choice = Choice::new("rust", "Rust")
            .with_annotation("detected in dev-deps")
            .recommended();
        assert_eq!(choice.id(), &ChoiceId::new("rust"));
        assert_eq!(choice.label(), "Rust");
        assert_eq!(choice.annotation(), Some("detected in dev-deps"));
        assert!(choice.is_recommended());

        let plain = Choice::new("go", "Go");
        assert_eq!(plain.annotation(), None);
        assert!(!plain.is_recommended());
    }
}
