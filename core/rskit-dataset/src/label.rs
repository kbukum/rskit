use serde::{Deserialize, Serialize};

/// Binary classification label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[repr(u8)]
pub enum Label {
    /// Human-authored / real sample.
    Real = 0,
    /// AI-generated sample.
    AiGenerated = 1,
}

impl std::fmt::Display for Label {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Label::Real => write!(f, "real"),
            Label::AiGenerated => write!(f, "ai"),
        }
    }
}
