use serde::{Deserialize, Serialize};

/// Cache store selection and common key settings.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CacheConfig {
    /// Store name looked up in an injected [`crate::CacheRegistry`].
    #[serde(default = "default_store", alias = "backend")]
    pub store: String,
    /// Optional prefix prepended to every key by stores that support it.
    pub key_prefix: Option<String>,
    /// In-memory store options.
    #[serde(default)]
    pub memory: MemoryConfig,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            store: default_store(),
            key_prefix: None,
            memory: MemoryConfig::default(),
        }
    }
}

fn default_store() -> String {
    "memory".to_owned()
}

/// In-memory cache configuration.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MemoryConfig {
    /// Optional maximum entry count. `Some(0)` is normalized to unbounded by the memory store.
    pub max_entries: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_expected_values() {
        let cfg = CacheConfig::default();
        assert_eq!(cfg.store, "memory");
        assert!(cfg.key_prefix.is_none());
        assert!(cfg.memory.max_entries.is_none());
    }

    #[test]
    fn deserialise_from_json() {
        let json = r#"{"store":"memory","memory":{"max_entries":2}}"#;
        let cfg: CacheConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.store, "memory");
        assert_eq!(cfg.memory.max_entries, Some(2));
    }

    #[test]
    fn deserialise_legacy_backend_field_from_json() {
        let json = r#"{"backend":"memory","memory":{"max_entries":2}}"#;
        let cfg: CacheConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.store, "memory");
        assert_eq!(cfg.memory.max_entries, Some(2));
    }
}
