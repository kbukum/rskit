//! Filesystem loading and activation for skill packs.

use std::fs::{self, File};
use std::io::{BufReader, Read};
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

const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_BODY_BYTES: u64 = 4 * 1024 * 1024;
const MAX_ASSET_BYTES: u64 = 16 * 1024 * 1024;
const MAX_ASSET_TOTAL_BYTES: u64 = 64 * 1024 * 1024;

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
    /// Non-fatal verification warnings observed while loading the pack.
    pub verification_warnings: Vec<String>,
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
        let (manifest, warnings) = self.load_metadata_with_warnings(root)?;
        for warning in warnings {
            tracing::warn!(warning = %warning, "skill verification warning");
        }
        Ok(manifest)
    }

    /// Activate a skill by loading body and inert asset inventory.
    pub fn activate(&self, root: impl AsRef<Path>) -> Result<Pack, SkillError> {
        let root = root.as_ref();
        let (manifest, verification_warnings) = self.load_metadata_with_warnings(root)?;
        for warning in &verification_warnings {
            tracing::warn!(warning = %warning, "skill verification warning");
        }
        let body_path = root.join(SKILL_MD_FILE_NAME);
        let body = read_utf8_bounded(&body_path, MAX_BODY_BYTES)?;
        let mut assets = Vec::new();
        let mut total_asset_bytes = 0;
        collect_assets(root.join("references"), &mut assets, &mut total_asset_bytes)?;
        collect_assets(root.join("scripts"), &mut assets, &mut total_asset_bytes)?;
        Ok(Pack {
            root: root.to_path_buf(),
            manifest,
            body: Some(body),
            assets,
            verification_warnings,
        })
    }

    /// Activate a skill using canonical `rskit-config` source resolution.
    pub fn activate_from_config(&self, loader: &ConfigLoader) -> Result<Pack, SkillError> {
        let config = loader
            .load::<SkillLoaderConfig>()
            .map_err(|error| SkillError::Config(error.to_string()))?;
        self.activate(config.root)
    }

    fn load_metadata_with_warnings(
        &self,
        root: impl AsRef<Path>,
    ) -> Result<(Manifest, Vec<String>), SkillError> {
        let root = root.as_ref();
        let manifest_path = root.join(MANIFEST_FILE_NAME);
        let data = read_utf8_bounded(&manifest_path, MAX_MANIFEST_BYTES)?;
        let manifest: Manifest =
            serde_norway::from_str(&data).map_err(|source| SkillError::ParseManifest {
                path: manifest_path,
                source,
            })?;
        manifest.validate()?;
        match self.verifier.verify(&manifest, root)? {
            VerificationOutcome::Denied(reason) => Err(SkillError::Verification(reason)),
            VerificationOutcome::Verified => Ok((manifest, Vec::new())),
            VerificationOutcome::Warning(warnings) => Ok((manifest, warnings)),
        }
    }
}

fn collect_assets(
    dir: PathBuf,
    assets: &mut Vec<Asset>,
    total_asset_bytes: &mut u64,
) -> Result<(), SkillError> {
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
            collect_assets(path, assets, total_asset_bytes)?;
        } else if path.is_file() {
            let digest = hash_file_bounded(&path, total_asset_bytes)?;
            assets.push(Asset {
                path,
                sha256: hex_lower(&digest),
            });
        }
    }
    assets.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(())
}

fn read_utf8_bounded(path: &Path, max_bytes: u64) -> Result<String, SkillError> {
    let file = File::open(path).map_err(|source| SkillError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut limited = file.take(max_bytes + 1);
    let mut bytes = Vec::new();
    limited
        .read_to_end(&mut bytes)
        .map_err(|source| SkillError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    if bytes.len() as u64 > max_bytes {
        return Err(SkillError::FileTooLarge {
            path: path.to_path_buf(),
            limit_bytes: max_bytes,
        });
    }
    String::from_utf8(bytes).map_err(|source| {
        SkillError::InvalidManifest(format!("{} is not valid UTF-8: {source}", path.display()))
    })
}

fn hash_file_bounded(
    path: &Path,
    total_asset_bytes: &mut u64,
) -> Result<sha2::digest::Output<Sha256>, SkillError> {
    let file = File::open(path).map_err(|source| SkillError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut reader = BufReader::new(file);
    let mut buffer = [0_u8; 16 * 1024];
    let mut hasher = Sha256::new();
    let mut file_bytes = 0_u64;

    loop {
        let read = reader.read(&mut buffer).map_err(|source| SkillError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if read == 0 {
            break;
        }
        file_bytes += read as u64;
        if file_bytes > MAX_ASSET_BYTES {
            return Err(SkillError::FileTooLarge {
                path: path.to_path_buf(),
                limit_bytes: MAX_ASSET_BYTES,
            });
        }
        *total_asset_bytes += read as u64;
        if *total_asset_bytes > MAX_ASSET_TOTAL_BYTES {
            return Err(SkillError::FileTooLarge {
                path: path.to_path_buf(),
                limit_bytes: MAX_ASSET_TOTAL_BYTES,
            });
        }
        hasher.update(&buffer[..read]);
    }

    Ok(hasher.finalize())
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
