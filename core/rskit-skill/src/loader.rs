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
#[non_exhaustive]
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

impl Pack {
    /// Create a pack from its root and manifest with no loaded body, assets, or warnings.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>, manifest: Manifest) -> Self {
        Self {
            root: root.into(),
            manifest,
            body: None,
            assets: Vec::new(),
            verification_warnings: Vec::new(),
        }
    }

    /// Attach loaded skill body content.
    #[must_use]
    pub fn with_body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

    /// Attach collected inert assets.
    #[must_use]
    pub fn with_assets(mut self, assets: Vec<Asset>) -> Self {
        self.assets = assets;
        self
    }

    /// Attach non-fatal verification warnings.
    #[must_use]
    pub fn with_verification_warnings(mut self, warnings: Vec<String>) -> Self {
        self.verification_warnings = warnings;
        self
    }
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
        let pack_root = canonicalize_path(root)?;
        let mut assets = Vec::new();
        let mut total_asset_bytes = 0;
        collect_assets(
            &pack_root,
            root.join("references"),
            &mut assets,
            &mut total_asset_bytes,
        )?;
        collect_assets(
            &pack_root,
            root.join("scripts"),
            &mut assets,
            &mut total_asset_bytes,
        )?;
        assets.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(Pack::new(root, manifest)
            .with_body(body)
            .with_assets(assets)
            .with_verification_warnings(verification_warnings))
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
    pack_root: &Path,
    dir: PathBuf,
    assets: &mut Vec<Asset>,
    total_asset_bytes: &mut u64,
) -> Result<(), SkillError> {
    let metadata = match fs::symlink_metadata(&dir) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(SkillError::Io { path: dir, source }),
    };
    reject_symlink(&dir, &metadata)?;
    if !metadata.is_dir() {
        return Err(invalid_pack_file(&dir, "expected directory"));
    }
    ensure_under_root(pack_root, &dir)?;

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
        let file_type = entry.file_type().map_err(|source| SkillError::Io {
            path: path.clone(),
            source,
        })?;
        if file_type.is_symlink() {
            return Err(invalid_pack_file(&path, "symlinks are not allowed"));
        }
        if file_type.is_dir() {
            ensure_under_root(pack_root, &path)?;
            collect_assets(pack_root, path, assets, total_asset_bytes)?;
        } else if file_type.is_file() {
            ensure_under_root(pack_root, &path)?;
            let digest = hash_file_bounded(&path, total_asset_bytes)?;
            assets.push(Asset {
                path,
                sha256: hex_lower(&digest),
            });
        } else {
            return Err(invalid_pack_file(
                &path,
                "expected regular file or directory",
            ));
        }
    }
    Ok(())
}

fn read_utf8_bounded(path: &Path, max_bytes: u64) -> Result<String, SkillError> {
    ensure_regular_file(path)?;
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
    String::from_utf8(bytes).map_err(|source| SkillError::InvalidUtf8 {
        path: path.to_path_buf(),
        source,
    })
}

fn hash_file_bounded(
    path: &Path,
    total_asset_bytes: &mut u64,
) -> Result<sha2::digest::Output<Sha256>, SkillError> {
    ensure_regular_file(path)?;
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

fn ensure_regular_file(path: &Path) -> Result<(), SkillError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| SkillError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    reject_symlink(path, &metadata)?;
    if metadata.is_file() {
        Ok(())
    } else {
        Err(invalid_pack_file(path, "expected regular file"))
    }
}

fn reject_symlink(path: &Path, metadata: &fs::Metadata) -> Result<(), SkillError> {
    if metadata.file_type().is_symlink() {
        Err(invalid_pack_file(path, "symlinks are not allowed"))
    } else {
        Ok(())
    }
}

fn ensure_under_root(pack_root: &Path, path: &Path) -> Result<(), SkillError> {
    let canonical = canonicalize_path(path)?;
    if canonical.starts_with(pack_root) {
        Ok(())
    } else {
        Err(invalid_pack_file(path, "path escapes skill pack root"))
    }
}

fn canonicalize_path(path: &Path) -> Result<PathBuf, SkillError> {
    path.canonicalize().map_err(|source| SkillError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn invalid_pack_file(path: &Path, reason: impl Into<String>) -> SkillError {
    SkillError::InvalidPackFile {
        path: path.to_path_buf(),
        reason: reason.into(),
    }
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
