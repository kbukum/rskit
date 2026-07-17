//! SDK-free skill manifests, loaders, registries, and verification contracts.

#![warn(missing_docs)]

mod constants;
mod error;
mod loader;
mod manifest;
mod registry;
mod verification;

pub use constants::{MANIFEST_FILE_NAME, SKILL_MD_FILE_NAME};
pub use error::SkillError;
pub use loader::{Asset, Loader, Pack, SkillLoaderConfig};
pub use manifest::{
    Budgets, HumanApprovalStep, Manifest, MaxCost, ModelHints, ProgressiveDisclosure, PromptRef,
    References, Requires, Safety, ScriptAsset, Signature,
};
pub use registry::{
    EffectiveEnvelope, InMemoryRegistry, Provider, Registry, effective_envelope, effective_safety,
    register_provider,
};
pub use verification::{DenyVerifier, VerificationOutcome, Verifier, WarnOnlyVerifier};

#[cfg(test)]
mod tests;
