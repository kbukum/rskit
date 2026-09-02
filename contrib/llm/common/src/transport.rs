//! Shared HTTP transport configuration for LLM provider adapters.
//!
//! Every provider config embeds [`HttpTransportConfig`] via `#[serde(default, flatten)]`,
//! so resilience, TLS, timeout, and header keys sit alongside the provider-specific fields
//! (`api_key`, `model`, …) in one config block. The vocabulary maps onto the canonical rskit
//! building blocks — [`rskit_httpclient::HttpClientConfig`], [`rskit_security::TlsConfig`], and
//! [`rskit_resilience`] — rather than introducing a parallel one, and mirrors the gokit
//! `llm.Config` transport keys for cross-kit intuition transfer.

use std::collections::BTreeMap;
use std::time::Duration;

use rskit_httpclient::HttpClientConfig;
use rskit_resilience::{Policy, RetryPolicy};
use rskit_security::TlsConfig;
use serde::{Deserialize, Serialize};

/// Optional HTTP transport tuning shared by every LLM provider adapter.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct HttpTransportConfig {
    /// Overall per-request timeout. Absent keeps the [`HttpClientConfig`] default.
    #[serde(
        with = "rskit_util::time::serde_duration::option",
        skip_serializing_if = "Option::is_none"
    )]
    pub timeout: Option<Duration>,

    /// Connection-establishment timeout. Absent keeps the [`HttpClientConfig`] default.
    #[serde(
        with = "rskit_util::time::serde_duration::option",
        skip_serializing_if = "Option::is_none"
    )]
    pub connect_timeout: Option<Duration>,

    /// Extra headers sent with every outbound request.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,

    /// TLS trust, client identity, and minimum-version configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls: Option<TlsConfig>,

    /// Retry/backoff policy applied to outbound requests. Absent disables retries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resilience: Option<RetryConfig>,
}

impl HttpTransportConfig {
    /// Layer these transport keys onto a base [`HttpClientConfig`], leaving unset keys untouched.
    #[must_use]
    pub fn apply_to(&self, mut config: HttpClientConfig) -> HttpClientConfig {
        if let Some(timeout) = self.timeout {
            config = config.with_timeout(timeout);
        }
        if let Some(connect_timeout) = self.connect_timeout {
            config = config.with_connect_timeout(connect_timeout);
        }
        for (name, value) in &self.headers {
            config = config.with_header(name, value);
        }
        if let Some(tls) = &self.tls {
            config = config.with_tls(tls.clone());
        }
        if let Some(policy) = self.resilience_policy() {
            config = config.with_resilience_policy(policy);
        }
        config
    }

    /// Build the outbound resilience [`Policy`], when a retry block is configured.
    #[must_use]
    pub fn resilience_policy(&self) -> Option<Policy> {
        self.resilience.as_ref().map(RetryConfig::to_policy)
    }
}

/// Retry/backoff vocabulary applied to outbound provider requests.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct RetryConfig {
    /// Maximum number of attempts, including the first call.
    pub max_attempts: usize,

    /// Delay before the first retry.
    #[serde(with = "rskit_util::time::serde_duration")]
    pub initial_backoff: Duration,

    /// Upper bound on any single backoff delay.
    #[serde(with = "rskit_util::time::serde_duration")]
    pub max_backoff: Duration,

    /// Multiplier applied on each successive retry for exponential backoff.
    pub backoff_factor: f64,

    /// Jitter fraction applied to each backoff delay, in `0.0..=1.0`.
    pub jitter: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(200),
            max_backoff: Duration::from_secs(10),
            backoff_factor: 2.0,
            jitter: 0.1,
        }
    }
}

impl RetryConfig {
    /// Translate the config vocabulary into a runtime resilience [`Policy`].
    #[must_use]
    pub fn to_policy(&self) -> Policy {
        Policy::new().with_retry(
            RetryPolicy::new()
                .with_max_attempts(self.max_attempts)
                .with_initial_backoff(self.initial_backoff)
                .with_max_backoff(self.max_backoff)
                .with_backoff_factor(self.backoff_factor)
                .with_jitter(self.jitter),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_defaults_are_empty_and_leave_client_config_untouched() {
        let transport = HttpTransportConfig::default();
        assert!(transport.timeout.is_none());
        assert!(transport.headers.is_empty());
        assert!(transport.tls.is_none());
        assert!(transport.resilience_policy().is_none());

        let cfg = transport.apply_to(HttpClientConfig::new());
        assert!(cfg.resilience_policy.is_none());
        assert!(cfg.default_headers.is_empty());
    }

    #[test]
    fn transport_deserializes_resilience_timeout_and_headers() {
        let json = r#"{
            "timeout": "45s",
            "connect_timeout": "5s",
            "headers": {"x-tenant": "acme"},
            "resilience": {"max_attempts": 4, "initial_backoff": "250ms"}
        }"#;
        let transport: HttpTransportConfig = serde_json::from_str(json).unwrap();

        assert_eq!(transport.timeout, Some(Duration::from_secs(45)));
        assert_eq!(transport.connect_timeout, Some(Duration::from_secs(5)));
        assert_eq!(
            transport.headers.get("x-tenant").map(String::as_str),
            Some("acme")
        );

        let policy = transport.resilience_policy().expect("resilience policy");
        assert!(policy.has_retry());

        let cfg = transport.apply_to(HttpClientConfig::new());
        assert_eq!(cfg.timeout, Duration::from_secs(45));
        assert_eq!(cfg.connect_timeout, Duration::from_secs(5));
        assert_eq!(
            cfg.default_headers.get("x-tenant").map(String::as_str),
            Some("acme")
        );
        assert!(cfg.resilience_policy.is_some());
    }

    #[test]
    fn transport_applies_tls_to_client_config() {
        let transport = HttpTransportConfig {
            tls: Some(TlsConfig::default()),
            ..HttpTransportConfig::default()
        };
        let cfg = transport.apply_to(HttpClientConfig::new());
        assert!(cfg.tls.is_some());
    }

    #[test]
    fn retry_config_defaults_build_a_retry_policy() {
        let policy = RetryConfig::default().to_policy();
        assert!(policy.has_retry());
    }
}
