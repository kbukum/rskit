use serde::{Deserialize, Serialize};

/// Cache store selection and common key settings.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CacheConfig {
    /// Store name looked up in an injected [`crate::CacheRegistry`].
    ///
    /// Serialized as the canonical `provider` field; the legacy `store` and `backend` names are
    /// still accepted on input for compatibility.
    #[serde(
        rename = "provider",
        default = "default_provider",
        alias = "store",
        alias = "backend"
    )]
    pub provider: String,
    /// Optional prefix prepended to every key by stores that support it.
    pub key_prefix: Option<String>,
    /// In-memory store options.
    #[serde(default)]
    pub memory: MemoryConfig,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            key_prefix: None,
            memory: MemoryConfig::default(),
        }
    }
}

fn default_provider() -> String {
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
        assert_eq!(cfg.provider, "memory");
        assert!(cfg.key_prefix.is_none());
        assert!(cfg.memory.max_entries.is_none());
    }

    #[test]
    fn deserialise_from_json() {
        let json = r#"{"provider":"memory","memory":{"max_entries":2}}"#;
        let cfg: CacheConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.provider, "memory");
        assert_eq!(cfg.memory.max_entries, Some(2));
    }

    #[test]
    fn deserialise_legacy_backend_field_from_json() {
        let json = r#"{"backend":"memory","memory":{"max_entries":2}}"#;
        let cfg: CacheConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.provider, "memory");
        assert_eq!(cfg.memory.max_entries, Some(2));
    }

    #[test]
    fn deserialise_legacy_store_field_from_json() {
        let json = r#"{"store":"redis"}"#;
        let cfg: CacheConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.provider, "redis");
    }

    #[test]
    fn serializes_canonical_provider_field_and_round_trips() {
        let cfg = CacheConfig {
            provider: "redis".to_owned(),
            key_prefix: Some("app".to_owned()),
            memory: MemoryConfig {
                max_entries: Some(8),
            },
        };
        let json = serde_json::to_value(&cfg).unwrap();
        assert_eq!(json["provider"], "redis");
        assert!(json.get("store").is_none());
        assert!(json.get("backend").is_none());

        let round_tripped: CacheConfig = serde_json::from_value(json).unwrap();
        assert_eq!(round_tripped.provider, cfg.provider);
        assert_eq!(round_tripped.key_prefix, cfg.key_prefix);
        assert_eq!(round_tripped.memory.max_entries, cfg.memory.max_entries);
    }
}
