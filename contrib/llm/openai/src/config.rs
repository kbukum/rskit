//! Configuration for the OpenAI provider.

use serde::Deserialize;

/// OpenAI provider configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// OpenAI API key.
    pub api_key: String,

    /// Base URL (default: `https://api.openai.com/v1`).
    #[serde(default = "default_base_url")]
    pub base_url: String,

    /// Chat model name (e.g. `gpt-4o`).
    #[serde(default = "default_model")]
    pub model: String,

    /// Embedding model name (e.g. `text-embedding-3-small`).
    #[serde(default = "default_embedding_model")]
    pub embedding_model: String,

    /// Embedding vector dimensions.
    #[serde(default = "default_embedding_dimensions")]
    pub embedding_dimensions: usize,
}

fn default_base_url() -> String {
    "https://api.openai.com/v1".into()
}

fn default_model() -> String {
    "gpt-4o".into()
}

fn default_embedding_model() -> String {
    "text-embedding-3-small".into()
}

fn default_embedding_dimensions() -> usize {
    1536
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_deserialize_with_defaults() {
        let json = r#"{"api_key":"sk-test"}"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.api_key, "sk-test");
        assert_eq!(cfg.base_url, "https://api.openai.com/v1");
        assert_eq!(cfg.model, "gpt-4o");
        assert_eq!(cfg.embedding_model, "text-embedding-3-small");
        assert_eq!(cfg.embedding_dimensions, 1536);
    }

    #[test]
    fn config_custom_values() {
        let json = r#"{
            "api_key":"sk-test",
            "base_url":"http://localhost:8080",
            "model":"gpt-3.5-turbo",
            "embedding_model":"text-embedding-ada-002",
            "embedding_dimensions":768
        }"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.base_url, "http://localhost:8080");
        assert_eq!(cfg.model, "gpt-3.5-turbo");
        assert_eq!(cfg.embedding_model, "text-embedding-ada-002");
        assert_eq!(cfg.embedding_dimensions, 768);
    }
}
