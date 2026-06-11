//! Qdrant adapter configuration.

use std::fmt;

use rskit_util::SecretString;
use rskit_vectorstore::SimilarityMetric;

/// Configuration for the Qdrant vector store.
#[derive(Clone)]
#[non_exhaustive]
pub struct Config {
    /// Qdrant server URL.
    pub url: String,
    /// Optional API key for Qdrant Cloud.
    pub api_key: Option<SecretString>,
    /// Metric used when creating collections.
    pub metric: SimilarityMetric,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("url", &"<redacted>")
            .field("api_key", &self.api_key)
            .field("metric", &self.metric)
            .finish()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            url: "http://localhost:6334".to_owned(),
            api_key: None,
            metric: SimilarityMetric::Cosine,
        }
    }
}

impl Config {
    /// Create a Qdrant adapter configuration for the given server URL.
    #[must_use]
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            ..Self::default()
        }
    }

    /// Set the optional Qdrant API key.
    #[must_use]
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(SecretString::new(api_key));
        self
    }

    /// Set the collection metric used for newly-created collections.
    #[must_use]
    pub const fn with_metric(mut self, metric: SimilarityMetric) -> Self {
        self.metric = metric;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_connection_details_and_api_key() {
        let config = Config {
            url: "https://qdrant.example.test:6334".to_owned(),
            api_key: Some(SecretString::new("super-secret")),
            metric: SimilarityMetric::Cosine,
        };

        let debug = format!("{config:?}");

        assert!(!debug.contains("qdrant.example.test"));
        assert!(!debug.contains("super-secret"));
        assert!(debug.contains("<redacted>"));
        assert!(debug.contains("SecretString(***)"));
    }

    #[test]
    fn builders_set_url_api_key_and_metric() {
        let config = Config::new("https://qdrant.example.test")
            .with_api_key("secret")
            .with_metric(SimilarityMetric::Dot);

        assert_eq!(config.url, "https://qdrant.example.test");
        assert_eq!(config.metric, SimilarityMetric::Dot);
        assert!(config.api_key.is_some());
    }
}
