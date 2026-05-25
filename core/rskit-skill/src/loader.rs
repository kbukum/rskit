//! Filesystem loading and activation for skill packs.

use std::fs;
use std::path::{Path, PathBuf};

use rskit_config::{AppConfig, ConfigLoader, ServiceConfig};
use rskit_validation::Validate;
use rskit_validation::validator::{ValidationError, ValidationErrors};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{
    MANIFEST_FILE_NAME, Manifest, SKILL_MD_FILE_NAME, SkillError, VerificationOutcome, Verifier,
    WarnOnlyVerifier,
};

/// Config-loader compatible skill activation source.
#[derive(Debug, Clone, Deserialize)]
pub struct SkillLoaderConfig {
    /// Embedded service config for canonical `rskit-config` loading.
    #[serde(default)]
    pub service: ServiceConfig,
    /// Root directory of the skill pack to activate.
    pub root: String,
}

impl Validate for SkillLoaderConfig {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        if let Err(error) = self.service.validate() {
            let mut validation_error = ValidationError::new("invalid_service");
            validation_error.message = Some(error.to_string().into());
            errors.add("service", validation_error);
        }
        if self.root.trim().is_empty() {
            errors.add("root", ValidationError::new("length"));
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl AppConfig for SkillLoaderConfig {
    fn apply_defaults(&mut self) {}

    fn service_config(&self) -> &ServiceConfig {
        &self.service
    }
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
            .map_err(|error| SkillError::Config(error.to_string()))?;
        self.activate(config.root)
    }
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
                sha256: hex_lower(&digest),
            });
        }
    }
    assets.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(())
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}
