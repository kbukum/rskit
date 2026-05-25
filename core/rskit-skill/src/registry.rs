//! Skill provider, registry, and effective envelope logic.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::{Manifest, Pack, Safety, SkillError};

/// Source of skill packs.
pub trait Provider: Send + Sync {
    /// Return metadata for available packs.
    fn manifests(&self) -> Result<Vec<Manifest>, SkillError>;
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
