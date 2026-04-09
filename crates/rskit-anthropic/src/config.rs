//! Configuration for the Anthropic provider.

use std::time::Duration;

use serde::Deserialize;

/// Configuration for the Anthropic provider.
#[derive(Debug, Clone, Deserialize)]
pub struct AnthropicConfig {
    pub api_key: String,
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default = "default_timeout", with = "humantime_serde")]
    pub timeout: Duration,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
}

fn default_base_url() -> String {
    "https://api.anthropic.com".into()
}

fn default_version() -> String {
    "2023-06-01".into()
}

fn default_timeout() -> Duration {
    Duration::from_secs(30)
}

fn default_max_retries() -> u32 {
    3
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
    fn anthropic_config_deserialise_with_defaults() {
        let json = r#"{"api_key":"sk-ant-test"}"#;
        let cfg: AnthropicConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.api_key, "sk-ant-test");
        assert_eq!(cfg.base_url, "https://api.anthropic.com");
        assert_eq!(cfg.version, "2023-06-01");
        assert_eq!(cfg.timeout, Duration::from_secs(30));
        assert_eq!(cfg.max_retries, 3);
    }

    #[test]
    fn anthropic_config_custom_values() {
        let json = r#"{"api_key":"key","base_url":"http://proxy","version":"2024-01-01","timeout":10,"max_retries":0}"#;
        let cfg: AnthropicConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.base_url, "http://proxy");
        assert_eq!(cfg.version, "2024-01-01");
        assert_eq!(cfg.timeout, Duration::from_secs(10));
        assert_eq!(cfg.max_retries, 0);
    }
}
