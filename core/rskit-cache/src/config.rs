use serde::{Deserialize, Serialize};

/// Cache backend selection and common key settings.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CacheConfig {
    /// Backend name looked up in an injected [`crate::CacheRegistry`].
    #[serde(default = "default_backend")]
    pub backend: String,
    /// Optional prefix prepended to every key by backends that support it.
    pub key_prefix: Option<String>,
    /// In-memory backend options.
    #[serde(default)]
    pub memory: MemoryConfig,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            backend: default_backend(),
            key_prefix: None,
            memory: MemoryConfig::default(),
        }
    }
}

fn default_backend() -> String {
    "memory".to_owned()
}

/// In-memory cache configuration.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MemoryConfig {
    /// Optional maximum entry count. `Some(0)` is normalized to unbounded by the memory backend.
    pub max_entries: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_expected_values() {
        let cfg = CacheConfig::default();
        assert_eq!(cfg.backend, "memory");
        assert!(cfg.key_prefix.is_none());
        assert!(cfg.memory.max_entries.is_none());
    }

    #[test]
    fn deserialise_from_json() {
        let json = r#"{"backend":"memory","memory":{"max_entries":2}}"#;
        let cfg: CacheConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.backend, "memory");
        assert_eq!(cfg.memory.max_entries, Some(2));
    }
}
