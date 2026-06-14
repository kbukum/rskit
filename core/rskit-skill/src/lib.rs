//! SDK-free skill manifests, loaders, registries, and verification contracts.

#![warn(missing_docs)]

mod error;
mod loader;
mod manifest;
mod registry;
mod verification;

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

/// Locked skill manifest filename.
pub const MANIFEST_FILE_NAME: &str = "kit.skill.yaml";
/// Progressive-disclosure body filename.
pub const SKILL_MD_FILE_NAME: &str = "SKILL.md";

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use rskit_config::ConfigLoader;
    use rskit_config::ServiceConfig;

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

    struct StaticWarningVerifier;

    impl Verifier for StaticWarningVerifier {
        fn verify(
            &self,
            _manifest: &Manifest,
            _root: &Path,
        ) -> Result<VerificationOutcome, SkillError> {
            Ok(VerificationOutcome::Warning(vec![
                "static warning".to_string(),
            ]))
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
    fn manifest_defaults_missing_human_approval_to_empty() {
        let manifest: Manifest = serde_norway::from_str(concat!(
            "schema_version: \"1\"\n",
            "name: demo\n",
            "version: 0.1.0\n",
            "description: Demo skill\n",
            "references: {}\n",
            "safety: read-only\n",
        ))
        .expect("manifest parses without human approval");

        assert!(manifest.human_approval.is_empty());
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
        assert!(
            pack.assets
                .windows(2)
                .all(|assets| assets[0].path <= assets[1].path)
        );
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
    fn loader_exposes_non_fatal_verification_warnings() {
        let root = test_root("warnings");
        let manifest = manifest("warned");
        write_pack(&root, &manifest);

        let pack = Loader::new(StaticWarningVerifier)
            .activate(&root)
            .expect("warning pack activates");

        assert_eq!(pack.verification_warnings, vec!["static warning"]);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn loader_reports_invalid_utf8_by_file() {
        let manifest_root = test_root("invalid-manifest-utf8");
        fs::write(manifest_root.join(MANIFEST_FILE_NAME), [0xff, 0xfe])
            .expect("write invalid manifest bytes");
        let error = Loader::default()
            .load_metadata(&manifest_root)
            .expect_err("invalid manifest utf8 rejected");
        assert!(
            matches!(error, SkillError::InvalidUtf8 { path, .. } if path.ends_with(MANIFEST_FILE_NAME))
        );
        fs::remove_dir_all(manifest_root).expect("cleanup manifest root");

        let body_root = test_root("invalid-body-utf8");
        write_pack(&body_root, &manifest("body-utf8"));
        fs::write(body_root.join(SKILL_MD_FILE_NAME), [0xff, 0xfe])
            .expect("write invalid body bytes");
        let error = Loader::default()
            .activate(&body_root)
            .expect_err("invalid body utf8 rejected");
        assert!(
            matches!(error, SkillError::InvalidUtf8 { path, .. } if path.ends_with(SKILL_MD_FILE_NAME))
        );
        fs::remove_dir_all(body_root).expect("cleanup body root");
    }

    #[test]
    fn loader_reports_manifest_directory_as_invalid_pack_file() {
        let root = test_root("manifest-directory");
        fs::create_dir(root.join(MANIFEST_FILE_NAME)).expect("create manifest directory");

        let error = Loader::default()
            .load_metadata(&root)
            .expect_err("manifest directory rejected");

        assert!(matches!(error, SkillError::InvalidPackFile { .. }));
        fs::remove_dir_all(root).expect("cleanup manifest root");
    }

    #[cfg(unix)]
    #[test]
    fn loader_rejects_pack_symlinks() {
        use std::os::unix::fs::symlink;

        let manifest_root = test_root("manifest-symlink");
        let manifest_target = manifest_root.with_extension("manifest.yml");
        fs::write(
            &manifest_target,
            serde_norway::to_string(&manifest("manifest-link")).expect("serialize manifest"),
        )
        .expect("write manifest target");
        symlink(&manifest_target, manifest_root.join(MANIFEST_FILE_NAME)).expect("link manifest");
        let error = Loader::default()
            .load_metadata(&manifest_root)
            .expect_err("manifest symlink rejected");
        assert!(matches!(error, SkillError::InvalidPackFile { .. }));
        fs::remove_dir_all(manifest_root).expect("cleanup manifest root");
        fs::remove_file(manifest_target).expect("cleanup manifest target");

        let body_root = test_root("body-symlink");
        write_pack(&body_root, &manifest("body-link"));
        let body_target = body_root.with_extension("skill.md");
        fs::write(&body_target, "# Linked body").expect("write body target");
        fs::remove_file(body_root.join(SKILL_MD_FILE_NAME)).expect("remove body");
        symlink(&body_target, body_root.join(SKILL_MD_FILE_NAME)).expect("link body");
        let error = Loader::default()
            .activate(&body_root)
            .expect_err("body symlink rejected");
        assert!(matches!(error, SkillError::InvalidPackFile { .. }));
        fs::remove_dir_all(body_root).expect("cleanup body root");
        fs::remove_file(body_target).expect("cleanup body target");

        let asset_root = test_root("asset-symlink");
        write_pack(&asset_root, &manifest("asset-link"));
        let asset_target = asset_root.with_extension("asset.txt");
        fs::write(&asset_target, "external").expect("write asset target");
        symlink(
            &asset_target,
            asset_root
                .join("references")
                .join("nested")
                .join("linked.txt"),
        )
        .expect("link asset");
        let error = Loader::default()
            .activate(&asset_root)
            .expect_err("asset symlink rejected");
        assert!(matches!(error, SkillError::InvalidPackFile { .. }));
        fs::remove_dir_all(asset_root).expect("cleanup asset root");
        fs::remove_file(asset_target).expect("cleanup asset target");
    }

    #[test]
    fn loader_rejects_oversized_manifest_and_body() {
        let manifest_limit = 1024 * 1024;
        let body_limit = 4 * 1024 * 1024;

        let manifest_root = test_root("oversized-manifest");
        fs::write(
            manifest_root.join(MANIFEST_FILE_NAME),
            vec![b'a'; manifest_limit + 1],
        )
        .expect("write oversized manifest");
        let error = Loader::default()
            .load_metadata(&manifest_root)
            .expect_err("oversized manifest rejected");
        assert!(matches!(error, SkillError::FileTooLarge { .. }));
        fs::remove_dir_all(manifest_root).expect("cleanup manifest root");

        let body_root = test_root("oversized-body");
        write_pack(&body_root, &manifest("body"));
        fs::write(
            body_root.join(SKILL_MD_FILE_NAME),
            vec![b'a'; body_limit + 1],
        )
        .expect("write oversized body");
        let error = Loader::default()
            .activate(&body_root)
            .expect_err("oversized body rejected");
        assert!(matches!(error, SkillError::FileTooLarge { .. }));
        fs::remove_dir_all(body_root).expect("cleanup body root");
    }

    #[test]
    fn loader_reports_config_failures_separately() {
        let error = Loader::default()
            .activate_from_config(&ConfigLoader::new())
            .expect_err("missing root config rejected");

        assert!(matches!(error, SkillError::Config(message) if message.contains("root")));
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
        let pack = Pack::new(PathBuf::from("demo"), manifest("demo"));
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

    #[test]
    fn manifest_rejects_script_paths_that_escape_the_pack() {
        for bad_path in ["../escape.sh", "/abs/escape.sh", "nested/../../escape.sh"] {
            let mut manifest = manifest("script-escape");
            manifest.scripts = vec![ScriptAsset {
                path: bad_path.to_string(),
                description: "untrusted script reference".to_string(),
            }];
            let error = manifest
                .validate()
                .expect_err("traversal script path rejected");
            assert!(
                matches!(error, SkillError::InvalidManifest(ref message) if message.contains("path")),
                "expected path rejection for {bad_path:?}, got {error:?}",
            );
        }
    }

    #[test]
    fn activate_rejects_asset_directory_that_is_actually_a_file() {
        let root = test_root("references-not-a-dir");
        fs::write(
            root.join(MANIFEST_FILE_NAME),
            serde_norway::to_string(&manifest("references-file")).expect("serialize manifest"),
        )
        .expect("write manifest");
        fs::write(root.join(SKILL_MD_FILE_NAME), "# Demo").expect("write body");
        // `references` is expected to be a directory; a regular file must fail closed.
        fs::write(root.join("references"), "not a directory").expect("write references file");

        let error = Loader::default()
            .activate(&root)
            .expect_err("non-directory references rejected");

        assert!(
            matches!(error, SkillError::InvalidPackFile { ref reason, .. } if reason.contains("expected directory")),
            "got {error:?}",
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn skill_loader_config_validation_requires_non_empty_root() {
        use rskit_validation::Validate;

        let blank = SkillLoaderConfig {
            service: ServiceConfig::default(),
            root: "   ".to_string(),
        };
        let errors = blank.validate().expect_err("blank root rejected");
        assert!(errors.field_errors().contains_key("root"));

        let populated = SkillLoaderConfig {
            service: ServiceConfig::default(),
            root: PathBuf::from("packs")
                .join("demo")
                .to_string_lossy()
                .into_owned(),
        };
        populated
            .validate()
            .expect("populated root with default service validates");
    }
}
