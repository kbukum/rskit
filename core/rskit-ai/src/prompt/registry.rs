//! Prompt registries and identities.

use std::collections::BTreeMap;

use semver::Version;
use serde::{Deserialize, Serialize};

use super::render::placeholders;
use super::template::{PromptError, PromptTemplate, VariableDecl, VariableType};

/// Stable prompt identity returned by registry listing.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PromptIdentity {
    /// Prompt name.
    pub name: String,
    /// Prompt version.
    pub version: Version,
}

/// Versioned prompt registry.
#[derive(Debug, Clone, Default)]
pub struct Registry {
    prompts: BTreeMap<(String, Version), PromptTemplate>,
}

impl Registry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a prompt template.
    pub fn register(
        &mut self,
        name: impl Into<String>,
        version: impl AsRef<str>,
        template: impl Into<String>,
        output_schema: Option<serde_json::Value>,
    ) -> Result<PromptTemplate, PromptError> {
        let name = name.into();
        let version_str = version.as_ref().to_owned();
        let version =
            Version::parse(&version_str).map_err(|source| PromptError::InvalidVersion {
                version: version_str,
                source,
            })?;
        if self.prompts.contains_key(&(name.clone(), version.clone())) {
            return Err(PromptError::AlreadyRegistered { name, version });
        }
        let body = template.into();
        let variables = placeholders(&body)
            .into_iter()
            .map(|name| VariableDecl {
                name,
                kind: VariableType::Any,
                required: true,
                default: None,
            })
            .collect::<Vec<_>>();
        let prompt = PromptTemplate {
            name: name.clone(),
            version: version.clone(),
            template: body,
            variables,
            output_schema,
            description: String::new(),
        };
        self.prompts.insert((name, version), prompt.clone());
        Ok(prompt)
    }

    /// Register an already-built prompt template.
    pub fn register_template(&mut self, prompt: PromptTemplate) -> Result<(), PromptError> {
        let key = (prompt.name.clone(), prompt.version.clone());
        if self.prompts.contains_key(&key) {
            return Err(PromptError::AlreadyRegistered {
                name: key.0,
                version: key.1,
            });
        }
        self.prompts.insert(key, prompt);
        Ok(())
    }

    /// Look up a prompt by exact version.
    pub fn lookup(&self, name: &str, version: &Version) -> Result<&PromptTemplate, PromptError> {
        self.prompts
            .get(&(name.to_owned(), version.clone()))
            .ok_or_else(|| PromptError::NotFound {
                name: name.to_owned(),
                version: version.clone(),
            })
    }

    /// Look up the highest semver for a prompt name.
    pub fn lookup_latest(&self, name: &str) -> Result<&PromptTemplate, PromptError> {
        self.prompts
            .iter()
            .rev()
            .find_map(|((prompt_name, _), prompt)| (prompt_name == name).then_some(prompt))
            .ok_or_else(|| PromptError::NameNotFound(name.to_owned()))
    }

    /// List prompt identities in stable order.
    #[must_use]
    pub fn list(&self) -> Vec<PromptIdentity> {
        self.prompts
            .keys()
            .map(|(name, version)| PromptIdentity {
                name: name.clone(),
                version: version.clone(),
            })
            .collect()
    }

    /// Return versions for one prompt name in ascending semver order.
    #[must_use]
    pub fn versions(&self, name: &str) -> Vec<Version> {
        self.prompts
            .keys()
            .filter(|(prompt_name, _)| prompt_name == name)
            .map(|(_, version)| version.clone())
            .collect()
    }
}
