//! HTTP client implementation.

use crate::config::HttpClientConfig;
use crate::request::{Request, RequestBody};
use crate::response::Response;
use reqwest::Client;
use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Async HTTP client with auth, headers, and error handling.
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
            .redirect(if config.follow_redirects {
                reqwest::redirect::Policy::limited(config.max_redirects)
            } else {
                reqwest::redirect::Policy::none()
            });

        if let Some(ua) = &config.user_agent {
            builder = builder.user_agent(ua.clone());
        }

        let client = builder.build().map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to build http client: {}", e),
            )
        })?;

        Ok(Self { client, config })
    }

    /// Gets the configuration.
    pub fn config(&self) -> &HttpClientConfig {
        &self.config
    }

    /// Executes an HTTP request.
    pub async fn send(&self, req: Request) -> AppResult<Response> {
        let url = self.build_url(&req.path)?;
        let mut request = match req.method.as_str() {
            "GET" => self.client.get(&url),
            "POST" => self.client.post(&url),
            "PUT" => self.client.put(&url),
            "PATCH" => self.client.patch(&url),
            "DELETE" => self.client.delete(&url),
            "HEAD" => self.client.head(&url),
            method => {
                return Err(AppError::new(
                    ErrorCode::InvalidInput,
                    format!("unsupported http method: {}", method),
                ));
            }
        };

        // Apply default headers
        for (name, value) in &self.config.default_headers {
            if let Ok(hv) = value.parse::<reqwest::header::HeaderValue>() {
                if let Ok(hn) = name.parse::<reqwest::header::HeaderName>() {
                    request = request.header(hn, hv);
                }
            }
        }

        // Apply request-specific headers
        for (name, value) in &req.headers {
            if let Ok(hv) = value.parse::<reqwest::header::HeaderValue>() {
                if let Ok(hn) = name.parse::<reqwest::header::HeaderName>() {
                    request = request.header(hn, hv);
                }
            }
        }

        // Apply query parameters
        if let Some(query) = &req.query {
            request = request.query(query);
        }

        // Apply authentication
        let mut headers = reqwest::header::HeaderMap::new();
        let auth = req.auth.as_ref().or(self.config.auth.as_ref());
        if let Some(a) = auth {
            a.apply(&mut headers);
        }
        for (name, value) in headers {
            if let Some(name) = name {
                request = request.header(name, value);
            }
        }

        // Apply body
        if let Some(body) = req.body {
            request = match body {
                RequestBody::Json(value) => request.json(&value),
                RequestBody::Text(text) => request.body(text),
                RequestBody::Bytes(bytes) => request.body(bytes),
            };
        }

        // Execute request
        let response = request.send().await.map_err(|e| {
            let code = if e.is_timeout() {
                ErrorCode::Timeout
            } else if e.is_connect() {
                ErrorCode::ConnectionFailed
            } else {
                ErrorCode::ExternalService
            };
            AppError::new(code, format!("http request failed: {}", e))
        })?;

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

        let body = response.bytes().await.map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("failed to read response body: {}", e),
            )
        })?;

        Ok(Response::new(status, headers, body))
    }

    /// Executes a GET request and returns the response.
    pub async fn get(&self, path: &str) -> AppResult<Response> {
        self.send(Request::get(path)).await
    }

    /// Executes a GET request and parses the response as JSON.
    pub async fn get_json<T: DeserializeOwned>(&self, path: &str) -> AppResult<T> {
        let resp = self.get(path).await?;
        resp.error_for_status()?.json()
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
        let resp = self.post(path, body).await?;
        resp.error_for_status()?.json()
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
        let resp = self.put(path, body).await?;
        resp.error_for_status()?.json()
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
        let resp = self.patch(path, body).await?;
        resp.error_for_status()?.json()
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
    fn build_url(&self, path: &str) -> AppResult<String> {
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
                .map(|u| u.to_string())
                .map_err(|e| AppError::new(ErrorCode::InvalidInput, format!("invalid url: {}", e)))
        } else {
            path.parse::<reqwest::Url>()
                .map(|u| u.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_building() {
        let config = HttpClientConfig::new().with_base_url("https://api.example.com/v1");
        let client = HttpClient::new(config).unwrap();

        let url = client.build_url("/users").unwrap();
        assert_eq!(url, "https://api.example.com/v1/users");

        let url = client.build_url("users").unwrap();
        assert_eq!(url, "https://api.example.com/v1/users");
    }

    #[test]
    fn test_url_building_without_base() {
        let config = HttpClientConfig::new();
        let client = HttpClient::new(config).unwrap();

        let url = client.build_url("https://example.com/users").unwrap();
        assert_eq!(url, "https://example.com/users");
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
}
