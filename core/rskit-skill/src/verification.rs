//! Manifest verification contracts and built-in verification policies.

use std::path::Path;

use crate::{Manifest, SkillError};

/// Verification result.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum VerificationOutcome {
    /// Verification passed.
    Verified,
    /// Verification emitted warnings and registration may continue.
    Warning(Vec<String>),
    /// Verification denied registration.
    Denied(String),
}

/// Verifier for manifest signatures.
pub trait Verifier: Send + Sync {
    /// Verify a manifest at load time.
    fn verify(&self, manifest: &Manifest, root: &Path) -> Result<VerificationOutcome, SkillError>;
}

/// Default warn-only verifier.
#[derive(Debug, Default, Clone, Copy)]
pub struct WarnOnlyVerifier;

impl Verifier for WarnOnlyVerifier {
    fn verify(&self, manifest: &Manifest, _root: &Path) -> Result<VerificationOutcome, SkillError> {
        if manifest.signature.is_some() {
            Ok(VerificationOutcome::Verified)
        } else {
            Ok(VerificationOutcome::Warning(vec![
                "unsigned skill manifest".to_string(),
            ]))
        }
    }
}

/// `DenyVerifier` is the canonical operator-deny verifier: it rejects every manifest unconditionally.
/// Use it as a safe default until a real signature verifier (e.g., Sigstore/cosign) adapter is wired in.
#[derive(Debug, Default, Clone, Copy)]
pub struct DenyVerifier;

impl Verifier for DenyVerifier {
    fn verify(
        &self,
        _manifest: &Manifest,
        _root: &Path,
    ) -> Result<VerificationOutcome, SkillError> {
        Ok(VerificationOutcome::Denied(
            "deny verifier: signatures rejected".to_string(),
        ))
    }
}
