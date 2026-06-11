//! HTTP client implementation.

use crate::config::HttpClientConfig;
use crate::request::{Request, RequestBody};
use crate::response::Response;
use std::error::Error;
use std::path::Path;

use reqwest::Client;
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_fs::sync_io::file;
use rskit_security::{TlsConfig, TlsVersion};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Async HTTP client with auth, headers, and error handling.
///
/// # Security (RS-ME-16 / #72)
/// SECURITY(#72): TLS certificate verification must never be disabled in production.
/// `danger_accept_invalid_certs` is only available behind the `danger-tls` feature flag
/// and must not be enabled in release builds.
#[derive(Clone)]
pub struct HttpClient {
    client: Client,
    config: HttpClientConfig,
}

impl HttpClient {
    /// Creates a new HTTP client with the given configuration.
    pub fn new(config: HttpClientConfig) -> AppResult<Self> {
        let mut builder = Client::builder()
            .timeout(config.timeout)
            .connect_timeout(config.connect_timeout)
            .redirect(redirect_policy(&config));

        if let Some(ua) = &config.user_agent {
            builder = builder.user_agent(ua.clone());
        }
        if let Some(tls) = &config.tls {
            builder = apply_tls(builder, tls)?;
        }

        let client = builder.build().map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to build http client: {}", e),
            )
        })?;

        Ok(Self { client, config })
    }

    /// Wraps an existing reqwest client with canonical configuration metadata.
    #[must_use]
    pub fn from_parts(config: HttpClientConfig, client: Client) -> Self {
        Self { client, config }
    }

    /// Gets the configuration.
    pub fn config(&self) -> &HttpClientConfig {
        &self.config
    }

    /// Executes an HTTP request.
    pub async fn send(&self, req: Request) -> AppResult<Response> {
        let mut response = self.execute_with_resilience(req).await?;

        let status = response.status();
        let headers = response
            .headers()
            .iter()
            .map(|(k, v)| {
                (
                    k.to_string(),
                    v.to_str().unwrap_or("<non-utf8>").to_string(),
                )
            })
            .collect();

        let body = read_response_body(&mut response, self.config.max_response_body_bytes).await?;

        Ok(Response::new(status, headers, body))
    }

    async fn execute_with_resilience(&self, req: Request) -> AppResult<reqwest::Response> {
        if let Some(policy) = &self.config.resilience_policy {
            policy
                .execute(|| async { self.execute_transport(req.clone()).await })
                .await
        } else {
            self.execute_transport(req).await
        }
    }

    async fn execute_transport(&self, req: Request) -> AppResult<reqwest::Response> {
        self.build_request(&req)?
            .send()
            .await
            .map_err(map_transport_error)
    }

    fn build_request(&self, req: &Request) -> AppResult<reqwest::RequestBuilder> {
        let url = self.build_url(&req.path)?;
        self.config.destination_policy.validate(&url)?;
        let mut request = match req.method.as_str() {
            "GET" => self.client.get(url),
            "POST" => self.client.post(url),
            "PUT" => self.client.put(url),
            "PATCH" => self.client.patch(url),
            "DELETE" => self.client.delete(url),
            "HEAD" => self.client.head(url),
            method => {
                return Err(AppError::new(
                    ErrorCode::InvalidInput,
                    format!("unsupported http method: {}", method),
                ));
            }
        };

        for (name, value) in &self.config.default_headers {
            let hn = parse_header_name(name)?;
            let hv = parse_header_value(name, value)?;
            request = request.header(hn, hv);
        }

        for (name, value) in &req.headers {
            let hn = parse_header_name(name)?;
            let hv = parse_header_value(name, value)?;
            request = request.header(hn, hv);
        }

        if let Some(query) = &req.query {
            request = request.query(query);
        }

        let auth = req.auth.as_ref().or(self.config.auth.as_ref());
        if let Some(auth) = auth
            && let Some((name, value)) = auth.header()?
        {
            let hn = parse_header_name(&name)?;
            let hv = parse_header_value(&name, &value)?;
            request = request.header(hn, hv);
        }

        if let Some(body) = &req.body {
            request = match body {
                RequestBody::Json(value) => request.json(value),
                RequestBody::Text(text) => request.body(text.clone()),
                RequestBody::Bytes(bytes) => request.body(bytes.clone()),
            };
        }

        Ok(request)
    }

    /// Executes a GET request and returns the response.
    pub async fn get(&self, path: &str) -> AppResult<Response> {
        self.send(Request::get(path)).await
    }

    /// Executes a request and converts non-2xx responses into an error.
    pub async fn send_checked(&self, req: Request) -> AppResult<Response> {
        self.send(req).await?.error_for_status()
    }

    /// Executes a GET request and parses the response as JSON.
    pub async fn get_json<T: DeserializeOwned>(&self, path: &str) -> AppResult<T> {
        self.get(path).await?.checked_json()
    }

    /// Executes a POST request with a JSON body.
    pub async fn post<T: Serialize>(&self, path: &str, body: &T) -> AppResult<Response> {
        let req = Request::post(path).json_body(body)?;
        self.send(req).await
    }

    /// Executes a POST request with a JSON body and parses the response as JSON.
    pub async fn post_json<T: Serialize, R: DeserializeOwned>(
        &self,
        path: &str,
        body: &T,
    ) -> AppResult<R> {
        self.post(path, body).await?.checked_json()
    }

    /// Executes a PUT request with a JSON body.
    pub async fn put<T: Serialize>(&self, path: &str, body: &T) -> AppResult<Response> {
        let req = Request::put(path).json_body(body)?;
        self.send(req).await
    }

    /// Executes a PUT request with a JSON body and parses the response as JSON.
    pub async fn put_json<T: Serialize, R: DeserializeOwned>(
        &self,
        path: &str,
        body: &T,
    ) -> AppResult<R> {
        self.put(path, body).await?.checked_json()
    }

    /// Executes a PATCH request with a JSON body.
    pub async fn patch<T: Serialize>(&self, path: &str, body: &T) -> AppResult<Response> {
        let req = Request::patch(path).json_body(body)?;
        self.send(req).await
    }

    /// Executes a PATCH request with a JSON body and parses the response as JSON.
    pub async fn patch_json<T: Serialize, R: DeserializeOwned>(
        &self,
        path: &str,
        body: &T,
    ) -> AppResult<R> {
        self.patch(path, body).await?.checked_json()
    }

    /// Executes a DELETE request.
    pub async fn delete(&self, path: &str) -> AppResult<Response> {
        self.send(Request::delete(path)).await
    }

    /// Executes a HEAD request.
    pub async fn head(&self, path: &str) -> AppResult<Response> {
        self.send(Request::head(path)).await
    }

    /// Builds the full URL from a path.
    fn build_url(&self, path: &str) -> AppResult<reqwest::Url> {
        if let Some(base) = &self.config.base_url {
            // Handle path to ensure correct joining
            let base_ends_slash = base.ends_with('/');
            let path_starts_slash = path.starts_with('/');

            let url = match (base_ends_slash, path_starts_slash) {
                (true, true) => format!("{}{}", base.trim_end_matches('/'), path),
                (true, false) | (false, true) => format!("{}{}", base, path),
                (false, false) => format!("{}/{}", base, path),
            };

            url.parse::<reqwest::Url>()
                .map_err(|e| AppError::new(ErrorCode::InvalidInput, format!("invalid url: {}", e)))
        } else {
            path.parse::<reqwest::Url>()
                .map_err(|e| AppError::new(ErrorCode::InvalidInput, format!("invalid url: {}", e)))
        }
    }
}

impl std::fmt::Debug for HttpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpClient")
            .field("config", &self.config)
            .finish()
    }
}

fn redirect_policy(config: &HttpClientConfig) -> reqwest::redirect::Policy {
    if !config.follow_redirects {
        return reqwest::redirect::Policy::none();
    }

    let max_redirects = config.max_redirects;
    let destination_policy = config.destination_policy.clone();
    reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() > max_redirects {
            return attempt.error(AppError::invalid_input(
                "max_redirects",
                format!("too many HTTP redirects (max {max_redirects})"),
            ));
        }
        if let Err(error) = destination_policy.validate(attempt.url()) {
            return attempt.error(error);
        }
        attempt.follow()
    })
}

async fn read_response_body(
    response: &mut reqwest::Response,
    max_bytes: usize,
) -> AppResult<bytes::Bytes> {
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes as u64)
    {
        return Err(response_body_too_large(max_bytes));
    }

    let mut total = 0usize;
    let mut body = bytes::BytesMut::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| {
        AppError::new(
            ErrorCode::ExternalService,
            format!("failed to read response body: {error}"),
        )
        .with_cause(error)
    })? {
        total = total
            .checked_add(chunk.len())
            .ok_or_else(|| response_body_too_large(max_bytes))?;
        if total > max_bytes {
            return Err(response_body_too_large(max_bytes));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body.freeze())
}

fn response_body_too_large(max_bytes: usize) -> AppError {
    AppError::invalid_input(
        "max_response_body_bytes",
        format!("HTTP response body exceeds configured limit of {max_bytes} bytes"),
    )
}

fn map_transport_error(error: reqwest::Error) -> AppError {
    if let Some(policy_error) = error
        .source()
        .and_then(|source| source.downcast_ref::<AppError>())
    {
        return AppError::new(policy_error.code(), policy_error.message()).with_cause(error);
    }

    let code = if error.is_timeout() {
        ErrorCode::Timeout
    } else if error.is_connect() {
        ErrorCode::ConnectionFailed
    } else {
        ErrorCode::ExternalService
    };
    AppError::new(code, format!("http request failed: {}", error))
}

fn apply_tls(
    mut builder: reqwest::ClientBuilder,
    tls: &TlsConfig,
) -> AppResult<reqwest::ClientBuilder> {
    tls.validate()?;
    if tls.server_name.is_some() {
        return Err(AppError::invalid_input(
            "tls.server_name",
            "HTTP client TLS server_name overrides are not supported by reqwest; omit the override so certificate verification uses the URL host",
        ));
    }

    builder = match tls.min_version {
        TlsVersion::Tls12 => builder.min_tls_version(reqwest::tls::Version::TLS_1_2),
        TlsVersion::Tls13 => builder.min_tls_version(reqwest::tls::Version::TLS_1_3),
        _ => builder.min_tls_version(reqwest::tls::Version::TLS_1_3),
    };

    builder = apply_skip_verify(builder, tls.skip_verify)?;

    if let Some(ca_file) = &tls.ca_file {
        let pem = file::read(Path::new(ca_file)).map_err(|error| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("failed to read HTTP CA bundle '{ca_file}': {error}"),
            )
            .with_cause(error)
        })?;
        let cert = reqwest::Certificate::from_pem(&pem).map_err(|error| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("invalid HTTP CA bundle '{ca_file}': {error}"),
            )
            .with_cause(error)
        })?;
        builder = builder.add_root_certificate(cert);
    }

    if let (Some(cert_file), Some(key_file)) = (&tls.cert_file, &tls.key_file) {
        let mut pem = file::read(Path::new(cert_file)).map_err(|error| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("failed to read HTTP client certificate '{cert_file}': {error}"),
            )
            .with_cause(error)
        })?;
        let mut key = file::read(Path::new(key_file)).map_err(|error| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("failed to read HTTP client key '{key_file}': {error}"),
            )
            .with_cause(error)
        })?;
        pem.append(&mut key);
        let identity = reqwest::Identity::from_pem(&pem).map_err(|error| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("invalid HTTP client identity '{cert_file}'/'{key_file}': {error}"),
            )
            .with_cause(error)
        })?;
        builder = builder.identity(identity);
    }

    Ok(builder)
}

fn apply_skip_verify(
    builder: reqwest::ClientBuilder,
    skip_verify: bool,
) -> AppResult<reqwest::ClientBuilder> {
    if !skip_verify {
        return Ok(builder);
    }

    #[cfg(all(feature = "danger-tls", debug_assertions))]
    {
        tracing::warn!("HTTP client TLS certificate verification disabled by explicit config");
        Ok(builder.danger_accept_invalid_certs(true))
    }

    #[cfg(not(all(feature = "danger-tls", debug_assertions)))]
    {
        Err(AppError::invalid_input(
            "tls.skip_verify",
            "HTTP client TLS certificate verification can only be disabled in debug builds with the danger-tls feature",
        ))
    }
}

fn parse_header_name(name: &str) -> AppResult<reqwest::header::HeaderName> {
    name.parse::<reqwest::header::HeaderName>()
        .map_err(|error| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("invalid HTTP header name '{name}': {error}"),
            )
        })
}

fn parse_header_value(name: &str, value: &str) -> AppResult<reqwest::header::HeaderValue> {
    value
        .parse::<reqwest::header::HeaderValue>()
        .map_err(|error| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("invalid HTTP header value for '{name}': {error}"),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_building() {
        let config = HttpClientConfig::new().with_base_url("https://api.example.com/v1");
        let client = HttpClient::new(config).unwrap();

        let url = client.build_url("/users").unwrap();
        assert_eq!(url.as_str(), "https://api.example.com/v1/users");

        let url = client.build_url("users").unwrap();
        assert_eq!(url.as_str(), "https://api.example.com/v1/users");
    }

    #[test]
    fn test_url_building_without_base() {
        let config = HttpClientConfig::new();
        let client = HttpClient::new(config).unwrap();

        let url = client.build_url("https://example.com/users").unwrap();
        assert_eq!(url.as_str(), "https://example.com/users");
    }

    #[test]
    fn test_client_creation() {
        let config = HttpClientConfig::new()
            .with_base_url("https://api.example.com")
            .with_user_agent("test-client/1.0");

        let client = HttpClient::new(config).unwrap();
        assert!(client.config.base_url.is_some());
        assert_eq!(
            client.config.user_agent,
            Some("test-client/1.0".to_string())
        );
    }

    #[test]
    fn from_parts_preserves_config_and_debug_uses_redacted_config() {
        let config = HttpClientConfig::new()
            .with_base_url("https://api.example.com")
            .with_auth(crate::Auth::bearer("secret-token"));
        let client = HttpClient::from_parts(config, reqwest::Client::new());

        assert_eq!(
            client.config().base_url.as_deref(),
            Some("https://api.example.com")
        );
        let debug = format!("{client:?}");
        assert!(debug.contains("HttpClient"));
        assert!(debug.contains("SecretString(***)"));
        assert!(!debug.contains("secret-token"));
    }

    #[test]
    fn base_url_joining_handles_all_slash_combinations() {
        let cases = [
            (
                "https://api.example.com/v1/",
                "/users",
                "https://api.example.com/v1/users",
            ),
            (
                "https://api.example.com/v1/",
                "users",
                "https://api.example.com/v1/users",
            ),
            (
                "https://api.example.com/v1",
                "/users",
                "https://api.example.com/v1/users",
            ),
            (
                "https://api.example.com/v1",
                "users",
                "https://api.example.com/v1/users",
            ),
        ];

        for (base, path, expected) in cases {
            let client = HttpClient::new(HttpClientConfig::new().with_base_url(base)).unwrap();

            assert_eq!(client.build_url(path).unwrap().as_str(), expected);
        }
    }

    #[test]
    fn tls_server_name_override_is_rejected() {
        let config = HttpClientConfig::new().with_tls(TlsConfig {
            server_name: Some("api.internal".to_string()),
            ..Default::default()
        });

        assert!(HttpClient::new(config).is_err());
    }

    #[test]
    fn tls_skip_verify_is_release_guarded() {
        let config = HttpClientConfig::new().with_tls(TlsConfig {
            skip_verify: true,
            ..Default::default()
        });

        let result = HttpClient::new(config);
        if cfg!(all(feature = "danger-tls", debug_assertions)) {
            assert!(result.is_ok());
        } else {
            assert!(result.is_err());
        }
    }

    #[test]
    fn tls13_minimum_is_accepted() {
        let config = HttpClientConfig::new().with_tls(TlsConfig {
            min_version: TlsVersion::Tls13,
            ..Default::default()
        });

        assert!(HttpClient::new(config).is_ok());
    }

    #[test]
    fn missing_ca_bundle_is_rejected() {
        let temp_dir = tempfile::tempdir().unwrap();
        let ca = temp_dir.path().join("missing-ca.pem");
        let config = HttpClientConfig::new().with_tls(TlsConfig {
            ca_file: Some(ca.display().to_string()),
            ..Default::default()
        });

        let error = HttpClient::new(config).expect_err("missing CA bundle");

        assert_eq!(error.code(), ErrorCode::InvalidInput);
        assert!(error.message().contains("failed to read HTTP CA bundle"));
    }

    #[test]
    fn missing_client_identity_files_are_rejected() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cert = temp_dir.path().join("client.pem");
        let key = temp_dir.path().join("client.key");
        let config = HttpClientConfig::new().with_tls(TlsConfig {
            cert_file: Some(cert.display().to_string()),
            key_file: Some(key.display().to_string()),
            ..Default::default()
        });

        let error = HttpClient::new(config).expect_err("missing identity files");

        assert_eq!(error.code(), ErrorCode::InvalidInput);
        assert!(
            error
                .message()
                .contains("failed to read HTTP client certificate")
        );
    }

    #[test]
    fn invalid_client_identity_is_rejected() {
        let cert = tempfile::NamedTempFile::new().unwrap();
        let key = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(cert.path(), b"not a cert").unwrap();
        std::fs::write(key.path(), b"not a key").unwrap();
        let config = HttpClientConfig::new().with_tls(TlsConfig {
            cert_file: Some(cert.path().display().to_string()),
            key_file: Some(key.path().display().to_string()),
            ..Default::default()
        });

        let error = HttpClient::new(config).expect_err("invalid identity");

        assert_eq!(error.code(), ErrorCode::InvalidInput);
        assert!(error.message().contains("invalid HTTP client identity"));
    }

    #[test]
    fn destination_policy_rejects_initial_url() {
        let config = HttpClientConfig::new();
        let client = HttpClient::new(config).unwrap();

        let result = client.build_request(&Request::get("http://169.254.169.254/latest"));

        assert!(result.is_err());
    }
}
