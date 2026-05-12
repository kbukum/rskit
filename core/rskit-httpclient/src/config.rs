//! HTTP client configuration.

use crate::auth::Auth;
use rskit_resilience::Policy;
use std::collections::HashMap;
use std::time::Duration;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Configuration for the HTTP client.
#[derive(Clone)]
pub struct HttpClientConfig {
    /// Base URL for requests (e.g., `https://api.example.com/v1`).
    /// Paths are appended to this URL.
    pub base_url: Option<String>,

    /// Request timeout. Defaults to 30 seconds.
    pub timeout: Duration,

    /// Connection timeout. Defaults to 10 seconds.
    pub connect_timeout: Duration,

    /// User-Agent header value. If None, no User-Agent header is set.
    pub user_agent: Option<String>,

    /// Default headers applied to all requests.
    pub default_headers: HashMap<String, String>,

    /// Default authentication applied to all requests.
    pub auth: Option<Auth>,

    /// Follow redirects. Defaults to true.
    pub follow_redirects: bool,

    /// Maximum number of redirects to follow. Defaults to 5.
    pub max_redirects: usize,

    /// Optional resilience policy applied to transport execution.
    pub resilience_policy: Option<Policy>,
}

impl std::fmt::Debug for HttpClientConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpClientConfig")
            .field("base_url", &self.base_url)
            .field("timeout", &self.timeout)
            .field("connect_timeout", &self.connect_timeout)
            .field("user_agent", &self.user_agent)
            .field("default_headers", &self.default_headers)
            .field("auth", &self.auth)
            .field("follow_redirects", &self.follow_redirects)
            .field("max_redirects", &self.max_redirects)
            .field("has_resilience_policy", &self.resilience_policy.is_some())
            .finish()
    }
}

impl HttpClientConfig {
    /// Creates a new HTTP client config with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the base URL.
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    /// Sets the timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Sets the connection timeout.
    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// Sets the User-Agent header.
    pub fn with_user_agent(mut self, ua: impl Into<String>) -> Self {
        self.user_agent = Some(ua.into());
        self
    }

    /// Adds a default header.
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.default_headers.insert(name.into(), value.into());
        self
    }

    /// Sets default headers.
    pub fn with_headers(mut self, headers: HashMap<String, String>) -> Self {
        self.default_headers = headers;
        self
    }

    /// Sets default authentication.
    pub fn with_auth(mut self, auth: Auth) -> Self {
        self.auth = Some(auth);
        self
    }

    /// Sets whether to follow redirects.
    pub fn with_follow_redirects(mut self, follow: bool) -> Self {
        self.follow_redirects = follow;
        self
    }

    /// Sets the maximum number of redirects to follow.
    pub fn with_max_redirects(mut self, max: usize) -> Self {
        self.max_redirects = max;
        self
    }

    /// Sets the transport resilience policy.
    #[must_use]
    pub fn with_resilience_policy(mut self, policy: Policy) -> Self {
        self.resilience_policy = Some(policy);
        self
    }
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            base_url: None,
            timeout: DEFAULT_TIMEOUT,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            user_agent: None,
            default_headers: HashMap::new(),
            auth: None,
            follow_redirects: true,
            max_redirects: 5,
            resilience_policy: None,
        }
    }
}
