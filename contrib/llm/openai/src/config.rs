//! Configuration for the `OpenAI` provider.

use rskit_llm_common::HttpTransportConfig;
use rskit_util::SecretString;
use serde::Deserialize;

/// `OpenAI` provider configuration.
#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    /// `OpenAI` API key.
    ///
    /// The value is redacted in debug/display output,
    /// and adapters pass it to `rskit-httpclient` as redacting auth state.
    pub api_key: SecretString,

    /// Base URL (default: `https://api.openai.com/v1`).
    #[serde(default = "default_base_url")]
    pub base_url: String,

    /// Chat model name (e.g. `gpt-4o`).
    #[serde(default = "default_model")]
    pub model: String,

    /// Embedding model name (e.g. `text-embedding-3-small`).
    #[serde(default = "default_embedding_model")]
    pub embedding_model: String,

    /// Optional embedding vector dimensions.
    ///
    /// Leave unset for models that do not accept a `dimensions` request field
    /// or when the model default dimensionality should be used.
    #[serde(default)]
    pub embedding_dimensions: Option<usize>,

    /// Optional HTTP transport tuning (timeout, headers, TLS, resilience).
    #[serde(default, flatten)]
    pub transport: HttpTransportConfig,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_deserialize_with_defaults() {
        let json = r#"{"api_key":"sk-test"}"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.api_key.expose(), "sk-test");
        assert_eq!(cfg.base_url, "https://api.openai.com/v1");
        assert_eq!(cfg.model, "gpt-4o");
        assert_eq!(cfg.embedding_model, "text-embedding-3-small");
        assert_eq!(cfg.embedding_dimensions, None);
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
        assert_eq!(cfg.embedding_dimensions, Some(768));
    }
}
