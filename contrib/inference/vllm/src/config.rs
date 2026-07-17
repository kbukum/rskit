use rskit_util::SecretString;
use serde::{Deserialize, Serialize};

/// Configuration for the vLLM OAI-compatible adapter.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    /// Base URL of the vLLM server, for example `http://localhost:8000`.
    pub base_url: String,
    /// Default model name if not provided in the request.
    #[serde(default = "default_model")]
    pub model: String,
    /// Optional bearer token for authenticated vLLM deployments.
    ///
    /// The value is redacted in debug output and serialization.
    #[serde(default)]
    pub api_key: Option<SecretString>,
    /// Max tokens for generation.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
}

fn default_model() -> String {
    "default".into()
}

fn default_max_tokens() -> u32 {
    256
}
