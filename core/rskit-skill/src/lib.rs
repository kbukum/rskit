//! SDK-free skill manifests, loaders, registries, and verification contracts.

#![warn(missing_docs)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::RwLock;
use rskit_config::{AppConfig, ConfigLoader, ServiceConfig};
use rskit_errors::AppError;
use rskit_validation::Validate;
use rskit_validation::Validator;
use rskit_validation::input::validate_safe_path;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Locked skill manifest filename.
pub const MANIFEST_FILE_NAME: &str = "kit.skill.yaml";
/// Progressive-disclosure body filename.
pub const SKILL_MD_FILE_NAME: &str = "SKILL.md";

/// Skill safety order. Informational in manifests; effective safety is computed from tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Safety {
    /// Read-only skill intent.
    #[default]
    ReadOnly,
    /// Mutating skill intent.
    Mutating,
    /// Destructive skill intent.
    Destructive,
}

/// Locked skill manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Canonical manifest schema version.
    #[serde(rename = "schema_version")]
    pub schema_version: String,
    /// Stable skill name.
    pub name: String,
    /// Semantic version.
    pub version: String,
    /// Human-readable description.
    pub description: String,
    /// Optional license expression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// Optional authors.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<String>,
    /// Referenced tools, resources, prompts, and MCP servers.
    pub references: References,
    /// Activation requirements.
    #[serde(default)]
    pub requires: Requires,
    /// Human approval checkpoints independent from tool sensitive invocations.
    #[serde(rename = "human_approval")]
    pub human_approval: Vec<HumanApprovalStep>,
    /// Optional budgets requested by the skill.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budgets: Option<Budgets>,
    /// Optional model routing hints.
    #[serde(
        default,
        rename = "model_hints",
        skip_serializing_if = "Option::is_none"
    )]
    pub model_hints: Option<ModelHints>,
    /// Progressive disclosure text.
    #[serde(
        default,
        rename = "progressive_disclosure",
        skip_serializing_if = "Option::is_none"
    )]
    pub progressive_disclosure: Option<ProgressiveDisclosure>,
    /// Inert script assets.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<ScriptAsset>,
    /// Optional signature metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<Signature>,
    /// Informational declared safety; does not grant authority.
    pub safety: Safety,
}

impl Manifest {
    /// Validate required string fields.
    pub fn validate(&self) -> Result<(), SkillError> {
        Validator::new()
            .required("schema_version", &self.schema_version)
            .required("name", &self.name)
            .required("version", &self.version)
            .required("description", &self.description)
            .validate()
            .map_err(|error| SkillError::InvalidManifest(error.to_string()))?;

        for script in &self.scripts {
            validate_safe_path(&script.path)
                .map_err(|error| SkillError::InvalidManifest(error.to_string()))?;
        }
        Ok(())
    }
}

/// Config-loader compatible skill activation source.
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct SkillLoaderConfig {
    /// Embedded service config for canonical `rskit-config` loading.
    #[serde(default)]
    #[validate(nested)]
    pub service: ServiceConfig,
    /// Root directory of the skill pack to activate.
    #[validate(length(min = 1))]
    pub root: String,
}

impl AppConfig for SkillLoaderConfig {
    fn apply_defaults(&mut self) {}

    fn service_config(&self) -> &ServiceConfig {
        &self.service
    }
}

/// Prompt reference with explicit version.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromptRef {
    /// Prompt name.
    pub name: String,
    /// Prompt version.
    pub version: String,
}

/// References to executable and context-bearing registrations by name/pattern.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct References {
    /// Tool names referenced by the skill.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
    /// Prompt names and versions referenced by the skill.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prompts: Vec<PromptRef>,
    /// Resource URI patterns referenced by the skill.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resources: Vec<String>,
    /// MCP server names referenced by the skill.
    #[serde(default, rename = "mcp_servers", skip_serializing_if = "Vec::is_empty")]
    pub mcp_servers: Vec<String>,
}

/// Activation preconditions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requires {
    /// Scopes the principal must hold to activate the skill. These never grant executable authority.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<String>,
    /// Capability gates such as network or filesystem.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
}

/// Human approval checkpoint.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HumanApprovalStep {
    /// Workflow step name.
    pub step: String,
    /// Human-readable condition.
    pub when: String,
    /// Why approval is required.
    pub rationale: String,
}

/// Skill-requested budget limits.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budgets {
    /// Maximum tokens.
    #[serde(
        default,
        rename = "max_tokens",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_tokens: Option<u64>,
    /// Maximum calls.
    #[serde(default, rename = "max_calls", skip_serializing_if = "Option::is_none")]
    pub max_calls: Option<u32>,
    /// Maximum cost.
    #[serde(default, rename = "max_cost", skip_serializing_if = "Option::is_none")]
    pub max_cost: Option<MaxCost>,
    /// ISO 8601 wall-clock duration.
    #[serde(
        default,
        rename = "wall_clock",
        skip_serializing_if = "Option::is_none"
    )]
    pub wall_clock: Option<String>,
}

/// Maximum cost budget.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaxCost {
    /// Decimal amount.
    pub amount: f64,
    /// Currency code.
    pub currency: String,
}

/// Optional model hints.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelHints {
    /// Ordered list of preferred model identifiers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preferred: Vec<String>,
    /// Model identifiers to reject.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reject: Vec<String>,
}

/// Signature metadata carried by the manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Signature {
    /// Signature algorithm or verifier hint.
    pub algorithm: String,
    /// Signature value.
    pub value: String,
    /// Verifier key identifier.
    #[serde(rename = "key_id")]
    pub key_id: String,
}

/// Progressive disclosure copy.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgressiveDisclosure {
    /// Short summary.
    pub summary: String,
    /// Detailed disclosure text.
    pub detail: String,
}

/// Inert script asset metadata.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptAsset {
    /// Script path relative to pack root.
    pub path: String,
    /// Script description.
    pub description: String,
}

/// Loaded skill pack.
#[derive(Debug, Clone)]
pub struct Pack {
    /// Pack root directory.
    pub root: PathBuf,
    /// Parsed manifest.
    pub manifest: Manifest,
    /// Body loaded from `SKILL.md` on activation.
    pub body: Option<String>,
    /// Inert asset inventory.
    pub assets: Vec<Asset>,
}

/// Inert asset recorded during activation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    /// Asset path.
    pub path: PathBuf,
    /// Lowercase hex SHA-256 digest.
    pub sha256: String,
}

/// Skill errors.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SkillError {
    /// File I/O failed.
    #[error("file I/O failed for {path}: {source}")]
    Io {
        /// File path.
        path: PathBuf,
        /// Source error.
        #[source]
        source: std::io::Error,
    },
    /// YAML parsing failed.
    #[error("manifest parse failed for {path}: {source}")]
    ParseManifest {
        /// Manifest path.
        path: PathBuf,
        /// Source error.
        #[source]
        source: serde_norway::Error,
    },
    /// Manifest is invalid.
    #[error("invalid skill manifest: {0}")]
    InvalidManifest(String),
    /// Verification failed.
    #[error("skill verification failed: {0}")]
    Verification(String),
    /// Registry conflict.
    #[error("skill already registered: {0}")]
    AlreadyRegistered(String),
    /// Skill not found.
    #[error("skill not found: {0}")]
    NotFound(String),
}

/// Verification result.
#[derive(Debug, Clone, PartialEq, Eq)]
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

/// `DenyVerifier` is the canonical operator-deny verifier: it rejects every
/// manifest unconditionally. Use it as a safe default until a real signature
/// verifier (e.g., Sigstore/cosign) adapter is wired in.
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

/// Filesystem loader for skill packs.
pub struct Loader<V = WarnOnlyVerifier> {
    verifier: V,
}

impl Default for Loader<WarnOnlyVerifier> {
    fn default() -> Self {
        Self {
            verifier: WarnOnlyVerifier,
        }
    }
}

impl<V: Verifier> Loader<V> {
    /// Create a loader with an injected verifier.
    pub fn new(verifier: V) -> Self {
        Self { verifier }
    }

    /// Load only manifest metadata.
    pub fn load_metadata(&self, root: impl AsRef<Path>) -> Result<Manifest, SkillError> {
        let root = root.as_ref();
        let manifest_path = root.join(MANIFEST_FILE_NAME);
        let data = fs::read_to_string(&manifest_path).map_err(|source| SkillError::Io {
            path: manifest_path.clone(),
            source,
        })?;
        let manifest: Manifest =
            serde_norway::from_str(&data).map_err(|source| SkillError::ParseManifest {
                path: manifest_path,
                source,
            })?;
        manifest.validate()?;
        match self.verifier.verify(&manifest, root)? {
            VerificationOutcome::Denied(reason) => Err(SkillError::Verification(reason)),
            VerificationOutcome::Verified | VerificationOutcome::Warning(_) => Ok(manifest),
        }
    }

    /// Activate a skill by loading body and inert asset inventory.
    pub fn activate(&self, root: impl AsRef<Path>) -> Result<Pack, SkillError> {
        let root = root.as_ref();
        let manifest = self.load_metadata(root)?;
        let body_path = root.join(SKILL_MD_FILE_NAME);
        let body = fs::read_to_string(&body_path).map_err(|source| SkillError::Io {
            path: body_path,
            source,
        })?;
        let mut assets = Vec::new();
        collect_assets(root.join("references"), &mut assets)?;
        collect_assets(root.join("scripts"), &mut assets)?;
        Ok(Pack {
            root: root.to_path_buf(),
            manifest,
            body: Some(body),
            assets,
        })
    }

    /// Activate a skill using canonical `rskit-config` source resolution.
    pub fn activate_from_config(&self, loader: &ConfigLoader) -> Result<Pack, SkillError> {
        let config = loader
            .load::<SkillLoaderConfig>()
            .map_err(|error| SkillError::InvalidManifest(error.to_string()))?;
        self.activate(config.root)
    }
}

/// Source of skill packs.
pub trait Provider: Send + Sync {
    /// Return metadata for available packs.
    fn manifests(&self) -> Result<Vec<Manifest>, SkillError>;
}

impl From<SkillError> for AppError {
    fn from(value: SkillError) -> Self {
        match value {
            SkillError::NotFound(name) => AppError::not_found("skill", Some(name.as_str())),
            SkillError::AlreadyRegistered(name) => {
                AppError::already_exists(format!("skill {name}"))
            }
            SkillError::InvalidManifest(message) | SkillError::Verification(message) => {
                AppError::invalid_input("skill", message)
            }
            SkillError::Io { path, source } => AppError::new(
                rskit_errors::ErrorCode::Internal,
                format!("skill I/O failed for {}: {source}", path.display()),
            )
            .with_cause(source),
            SkillError::ParseManifest { path, source } => AppError::new(
                rskit_errors::ErrorCode::InvalidInput,
                format!(
                    "skill manifest parse failed for {}: {source}",
                    path.display()
                ),
            )
            .with_cause(source),
        }
    }
}

/// Registry abstraction for skills.
pub trait Registry: Send + Sync {
    /// Register one pack.
    fn register(&self, pack: Pack) -> Result<(), SkillError>;
    /// Get a pack by name.
    fn get(&self, name: &str) -> Option<Pack>;
    /// List registered manifests.
    fn list(&self) -> Vec<Manifest>;
}

/// In-memory registry implementation.
#[derive(Default)]
pub struct InMemoryRegistry {
    packs: RwLock<BTreeMap<String, Arc<Pack>>>,
}

impl InMemoryRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Registry for InMemoryRegistry {
    fn register(&self, pack: Pack) -> Result<(), SkillError> {
        let name = pack.manifest.name.clone();
        let mut packs = self.packs.write();
        if packs.contains_key(&name) {
            return Err(SkillError::AlreadyRegistered(name));
        }
        packs.insert(name, Arc::new(pack));
        Ok(())
    }

    fn get(&self, name: &str) -> Option<Pack> {
        self.packs.read().get(name).map(|pack| (**pack).clone())
    }

    fn list(&self) -> Vec<Manifest> {
        self.packs
            .read()
            .values()
            .map(|pack| pack.manifest.clone())
            .collect()
    }
}

/// Explicitly register all packs from a provider into a registry.
pub fn register_provider(
    provider: &dyn Provider,
    registry: &dyn Registry,
) -> Result<(), SkillError> {
    for manifest in provider.manifests()? {
        let pack = Pack {
            root: PathBuf::new(),
            manifest,
            body: None,
            assets: Vec::new(),
        };
        registry.register(pack)?;
    }
    Ok(())
}

/// Compute effective safety as the maximum over referenced tool envelope safety values.
pub fn effective_safety(safeties: impl IntoIterator<Item = Safety>) -> Safety {
    safeties.into_iter().max().unwrap_or(Safety::ReadOnly)
}

/// Compute a conservative effective envelope by intersecting scope sets and maxing safety.
pub fn effective_envelope(
    declared_scopes: &[String],
    principal_grants: &[String],
    operator_ceiling: &[String],
    referenced: bool,
    safety: Safety,
) -> EffectiveEnvelope {
    if !referenced {
        return EffectiveEnvelope {
            scopes: Vec::new(),
            safety: Safety::ReadOnly,
            active: false,
        };
    }
    let scopes = declared_scopes
        .iter()
        .filter(|scope| principal_grants.contains(scope) && operator_ceiling.contains(scope))
        .cloned()
        .collect();
    EffectiveEnvelope {
        scopes,
        safety,
        active: true,
    }
}

/// Minimal effective envelope result used by activation code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveEnvelope {
    /// Intersected executable scopes.
    pub scopes: Vec<String>,
    /// Effective safety.
    pub safety: Safety,
    /// Whether the referenced tool is active.
    pub active: bool,
}

fn collect_assets(dir: PathBuf, assets: &mut Vec<Asset>) -> Result<(), SkillError> {
    if !dir.exists() {
        return Ok(());
    }
    let entries = fs::read_dir(&dir).map_err(|source| SkillError::Io {
        path: dir.clone(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| SkillError::Io {
            path: dir.clone(),
            source,
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_assets(path, assets)?;
        } else if path.is_file() {
            let data = fs::read(&path).map_err(|source| SkillError::Io {
                path: path.clone(),
                source,
            })?;
            let digest = Sha256::digest(&data);
            assets.push(Asset {
                path,
                sha256: format!("{digest:x}"),
            });
        }
    }
    assets.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StaticProvider {
        manifests: Vec<Manifest>,
    }

    impl Provider for StaticProvider {
        fn manifests(&self) -> Result<Vec<Manifest>, SkillError> {
            Ok(self.manifests.clone())
        }
    }

    struct BlockingVerifier;

    impl Verifier for BlockingVerifier {
        fn verify(
            &self,
            _manifest: &Manifest,
            _root: &Path,
        ) -> Result<VerificationOutcome, SkillError> {
            Ok(VerificationOutcome::Denied("blocked".to_string()))
        }
    }

    fn manifest(name: &str) -> Manifest {
        Manifest {
            schema_version: "1".to_string(),
            name: name.to_string(),
            version: "0.1.0".to_string(),
            description: "Demo skill".to_string(),
            license: Some("MIT".to_string()),
            authors: vec!["rskit contributors".to_string()],
            references: References {
                tools: vec!["search".to_string()],
                prompts: vec![PromptRef {
                    name: "summarize".to_string(),
                    version: "1.0.0".to_string(),
                }],
                resources: vec!["docs/**".to_string()],
                mcp_servers: vec!["local".to_string()],
            },
            requires: Requires {
                scopes: vec!["tool:search".to_string(), "docs:read".to_string()],
                capabilities: vec!["network".to_string()],
            },
            human_approval: vec![HumanApprovalStep {
                step: "publish".to_string(),
                when: "before publishing".to_string(),
                rationale: "external side effect".to_string(),
            }],
            budgets: Some(Budgets {
                max_tokens: Some(1024),
                max_calls: Some(3),
                max_cost: Some(MaxCost {
                    amount: 1.25,
                    currency: "USD".to_string(),
                }),
                wall_clock: Some("PT30S".to_string()),
            }),
            model_hints: Some(ModelHints {
                preferred: vec!["gpt-5-mini".to_string()],
                reject: vec!["legacy".to_string()],
            }),
            progressive_disclosure: Some(ProgressiveDisclosure {
                summary: "Demo".to_string(),
                detail: "Use this skill.".to_string(),
            }),
            scripts: vec![ScriptAsset {
                path: "scripts/helper.sh".to_string(),
                description: "inert helper".to_string(),
            }],
            signature: Some(Signature {
                algorithm: "ed25519".to_string(),
                value: "sig".to_string(),
                key_id: "test-key".to_string(),
            }),
            safety: Safety::Mutating,
        }
    }

    fn test_root(name: &str) -> PathBuf {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("skill-tests")
            .join(format!("{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create test root");
        root
    }

    fn write_pack(root: &Path, manifest: &Manifest) {
        fs::write(
            root.join(MANIFEST_FILE_NAME),
            serde_norway::to_string(manifest).expect("serialize manifest"),
        )
        .expect("write manifest");
        fs::write(root.join(SKILL_MD_FILE_NAME), "# Demo\nUse this skill.").expect("write body");
        let references = root.join("references").join("nested");
        fs::create_dir_all(&references).expect("create references");
        fs::write(references.join("note.txt"), "reference").expect("write reference");
        let scripts = root.join("scripts");
        fs::create_dir_all(&scripts).expect("create scripts");
        fs::write(scripts.join("helper.sh"), "echo inert").expect("write script");
    }

    #[test]
    fn computes_max_safety() {
        assert_eq!(
            effective_safety([Safety::ReadOnly, Safety::Destructive, Safety::Mutating]),
            Safety::Destructive
        );
        assert_eq!(effective_safety([]), Safety::ReadOnly);
    }

    #[test]
    fn computes_effective_envelope() {
        let envelope = effective_envelope(
            &["tool:search".to_string(), "admin".to_string()],
            &["tool:search".to_string(), "docs:read".to_string()],
            &["tool:search".to_string()],
            true,
            Safety::Mutating,
        );
        assert_eq!(envelope.scopes, vec!["tool:search"]);
        assert_eq!(envelope.safety, Safety::Mutating);
        assert!(envelope.active);

        let inactive = effective_envelope(
            &["tool:search".to_string()],
            &["tool:search".to_string()],
            &["tool:search".to_string()],
            false,
            Safety::Destructive,
        );
        assert_eq!(inactive.scopes, Vec::<String>::new());
        assert_eq!(inactive.safety, Safety::ReadOnly);
        assert!(!inactive.active);
    }

    #[test]
    fn parses_manifest_yaml() {
        let manifest: Manifest = serde_norway::from_str(
            r#"
schema_version: "1"
name: demo
version: 0.1.0
description: Demo skill
license: MIT
authors: [Ada]
references:
  tools: [search]
  prompts:
    - name: summarize
      version: 1.0.0
  resources: [docs]
  mcp_servers: [local]
requires:
  scopes: [tool:search]
  capabilities: [network, filesystem]
human_approval:
  - step: publish
    when: before publishing
    rationale: external side effect
budgets:
  max_tokens: 1024
  max_calls: 3
  max_cost: {amount: 1.25, currency: USD}
  wall_clock: PT60S
model_hints:
  preferred: [gpt-5-mini]
  reject: [legacy]
progressive_disclosure:
  summary: Demo
  detail: Details
scripts:
  - path: scripts/helper.sh
    description: inert helper
signature:
  algorithm: ed25519
  value: sig
  key_id: test-key
safety: read-only
"#,
        )
        .expect("manifest parses");
        manifest.validate().expect("manifest validates");
        assert_eq!(manifest.references.tools, vec!["search"]);
        assert_eq!(manifest.references.prompts[0].version, "1.0.0");
        assert_eq!(manifest.human_approval[0].step, "publish");
        assert_eq!(
            manifest.signature.as_ref().expect("signature").key_id,
            "test-key"
        );
    }

    #[test]
    fn rejects_invalid_manifests() {
        let mut manifest = manifest("demo");
        manifest.name.clear();
        let error = manifest.validate().expect_err("empty name rejected");
        assert!(matches!(error, SkillError::InvalidManifest(message) if message.contains("name")));

        let root = test_root("parse-error");
        fs::write(root.join(MANIFEST_FILE_NAME), "name: [bad").expect("write malformed yaml");
        let error = Loader::default()
            .load_metadata(&root)
            .expect_err("malformed yaml rejected");
        assert!(matches!(error, SkillError::ParseManifest { .. }));
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn loader_verifies_and_activates_pack() {
        let root = test_root("activate");
        let manifest = manifest("demo");
        write_pack(&root, &manifest);

        let loaded = Loader::default()
            .load_metadata(&root)
            .expect("metadata loads");
        assert_eq!(loaded.name, "demo");

        let pack = Loader::default().activate(&root).expect("pack activates");
        assert_eq!(pack.root, root);
        assert_eq!(pack.body.as_deref(), Some("# Demo\nUse this skill."));
        assert_eq!(pack.assets.len(), 2);
        assert!(pack.assets.iter().all(|asset| asset.sha256.len() == 64));
        fs::remove_dir_all(&pack.root).expect("cleanup");
    }

    #[test]
    fn loader_reports_io_and_verification_failures() {
        let root = test_root("failures");
        let missing = Loader::default()
            .load_metadata(&root)
            .expect_err("missing manifest rejected");
        assert!(matches!(missing, SkillError::Io { .. }));

        let manifest = manifest("denied");
        write_pack(&root, &manifest);
        let denied = Loader::new(BlockingVerifier)
            .load_metadata(&root)
            .expect_err("denied manifest rejected");
        assert!(matches!(denied, SkillError::Verification(message) if message == "blocked"));
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn verifiers_return_expected_outcomes() {
        let unsigned = Manifest {
            signature: None,
            ..manifest("unsigned")
        };
        let warning = WarnOnlyVerifier
            .verify(&unsigned, Path::new("."))
            .expect("warn verifier succeeds");
        assert!(
            matches!(warning, VerificationOutcome::Warning(messages) if messages == vec!["unsigned skill manifest"])
        );

        let verified = WarnOnlyVerifier
            .verify(&manifest("signed"), Path::new("."))
            .expect("signed manifest verifies");
        assert_eq!(verified, VerificationOutcome::Verified);

        let denied = DenyVerifier
            .verify(&manifest("signed"), Path::new("."))
            .expect("deny verifier returns denial");
        assert!(
            matches!(denied, VerificationOutcome::Denied(message) if message.contains("deny verifier"))
        );
    }

    #[test]
    fn registry_registers_lists_and_rejects_duplicates() {
        let registry = InMemoryRegistry::new();
        let pack = Pack {
            root: PathBuf::from("demo"),
            manifest: manifest("demo"),
            body: None,
            assets: Vec::new(),
        };
        registry.register(pack.clone()).expect("register pack");
        assert_eq!(
            registry.get("demo").expect("get pack").manifest.name,
            "demo"
        );
        assert_eq!(registry.list().len(), 1);
        assert!(registry.get("missing").is_none());
        let duplicate = registry.register(pack).expect_err("duplicate rejected");
        assert!(matches!(duplicate, SkillError::AlreadyRegistered(name) if name == "demo"));
    }

    #[test]
    fn register_provider_loads_manifest_packs() {
        let registry = InMemoryRegistry::new();
        let provider = StaticProvider {
            manifests: vec![manifest("alpha"), manifest("beta")],
        };
        register_provider(&provider, &registry).expect("provider registers");
        let names = registry
            .list()
            .into_iter()
            .map(|manifest| manifest.name)
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["alpha", "beta"]);
    }
}
