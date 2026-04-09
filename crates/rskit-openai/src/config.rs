//! Configuration for OpenAI providers.

use std::time::Duration;

use serde::Deserialize;

/// Configuration for the OpenAI LLM provider.
#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiConfig {
    pub api_key: String,
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default = "default_timeout", with = "humantime_serde")]
    pub timeout: Duration,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
}

fn default_base_url() -> String {
    "https://api.openai.com/v1".into()
}

fn default_timeout() -> Duration {
    Duration::from_secs(30)
}

fn default_max_retries() -> u32 {
    3
}

/// Configuration for the OpenAI-compatible embedding provider.
#[derive(Debug, Clone)]
pub struct OpenAiEmbeddingConfig {
    /// Base URL for the API (e.g., `https://api.openai.com`).
    pub endpoint: String,
    /// API key for authentication. Empty string disables the header.
    pub api_key: String,
    /// Model name (e.g., `text-embedding-3-small`).
    pub model: String,
    /// Expected embedding dimensions.
    pub dimensions: usize,
}

impl Default for OpenAiEmbeddingConfig {
    fn default() -> Self {
        Self {
            endpoint: "https://api.openai.com".to_owned(),
            api_key: String::new(),
            model: "text-embedding-3-small".to_owned(),
            dimensions: 1536,
        }
    }
}

/// Serde helper module for `Duration` using seconds as u64.
mod humantime_serde {
    use std::time::Duration;

    use serde::{self, Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(secs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_config_deserialise_with_defaults() {
        let json = r#"{"api_key":"sk-test"}"#;
        let cfg: OpenAiConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.api_key, "sk-test");
        assert_eq!(cfg.base_url, "https://api.openai.com/v1");
        assert_eq!(cfg.timeout, Duration::from_secs(30));
        assert_eq!(cfg.max_retries, 3);
    }

    #[test]
    fn openai_config_custom_base_url() {
        let json = r#"{"api_key":"sk-test","base_url":"http://localhost:8080","timeout":60,"max_retries":1}"#;
        let cfg: OpenAiConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.base_url, "http://localhost:8080");
        assert_eq!(cfg.timeout, Duration::from_secs(60));
        assert_eq!(cfg.max_retries, 1);
    }

    #[test]
    fn embedding_config_defaults() {
        let cfg = OpenAiEmbeddingConfig::default();
        assert_eq!(cfg.endpoint, "https://api.openai.com");
        assert_eq!(cfg.model, "text-embedding-3-small");
        assert_eq!(cfg.dimensions, 1536);
    }
}
