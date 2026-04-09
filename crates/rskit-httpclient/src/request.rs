//! HTTP request builder.

use crate::auth::Auth;
use http::Method;
use std::collections::HashMap;

/// HTTP request builder.
#[derive(Debug, Clone)]
pub struct Request {
    /// HTTP method (GET, POST, etc.)
    pub method: Method,

    /// Request path (absolute or relative to base_url)
    pub path: String,

    /// Request headers
    pub headers: HashMap<String, String>,

    /// Request body (JSON or bytes)
    pub body: Option<RequestBody>,

    /// Query parameters
    pub query: Option<HashMap<String, String>>,

    /// Authentication override (if None, use client default)
    pub auth: Option<Auth>,
}

/// Request body.
#[derive(Debug, Clone)]
pub enum RequestBody {
    /// JSON body (serialized via serde_json)
    Json(serde_json::Value),
    /// Text body
    Text(String),
    /// Binary body
    Bytes(bytes::Bytes),
}

impl RequestBody {
    /// Creates a JSON body from a serializable value.
    pub fn json<T: serde::Serialize>(value: &T) -> Result<Self, serde_json::Error> {
        Ok(RequestBody::Json(serde_json::to_value(value)?))
    }

    /// Creates a text body.
    pub fn text(text: impl Into<String>) -> Self {
        RequestBody::Text(text.into())
    }

    /// Creates a binary body.
    pub fn bytes(bytes: impl Into<bytes::Bytes>) -> Self {
        RequestBody::Bytes(bytes.into())
    }
}

impl Request {
    /// Creates a new GET request.
    pub fn get(path: impl Into<String>) -> Self {
        Self::new(Method::GET, path)
    }

    /// Creates a new POST request.
    pub fn post(path: impl Into<String>) -> Self {
        Self::new(Method::POST, path)
    }

    /// Creates a new PUT request.
    pub fn put(path: impl Into<String>) -> Self {
        Self::new(Method::PUT, path)
    }

    /// Creates a new PATCH request.
    pub fn patch(path: impl Into<String>) -> Self {
        Self::new(Method::PATCH, path)
    }

    /// Creates a new DELETE request.
    pub fn delete(path: impl Into<String>) -> Self {
        Self::new(Method::DELETE, path)
    }

    /// Creates a new HEAD request.
    pub fn head(path: impl Into<String>) -> Self {
        Self::new(Method::HEAD, path)
    }

    /// Creates a new request with the given method and path.
    pub fn new(method: Method, path: impl Into<String>) -> Self {
        Self {
            method,
            path: path.into(),
            headers: HashMap::new(),
            body: None,
            query: None,
            auth: None,
        }
    }

    /// Sets the request path.
    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    /// Adds a header.
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }

    /// Sets multiple headers.
    pub fn headers(mut self, headers: HashMap<String, String>) -> Self {
        self.headers = headers;
        self
    }

    /// Sets the request body.
    pub fn body(mut self, body: RequestBody) -> Self {
        self.body = Some(body);
        self
    }

    /// Sets the request body from a JSON-serializable value.
    pub fn json_body<T: serde::Serialize>(mut self, value: &T) -> Result<Self, serde_json::Error> {
        self.body = Some(RequestBody::json(value)?);
        Ok(self)
    }

    /// Sets query parameters.
    pub fn query(mut self, params: HashMap<String, String>) -> Self {
        self.query = Some(params);
        self
    }

    /// Adds a query parameter.
    pub fn query_param(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        if self.query.is_none() {
            self.query = Some(HashMap::new());
        }
        if let Some(ref mut q) = self.query {
            q.insert(name.into(), value.into());
        }
        self
    }

    /// Sets authentication for this request (overrides client default).
    pub fn auth(mut self, auth: Auth) -> Self {
        self.auth = Some(auth);
        self
    }

    /// Sets Bearer token authentication for this request.
    pub fn bearer_token(self, token: impl Into<String>) -> Self {
        self.auth(Auth::bearer(token))
    }

    /// Sets Basic authentication for this request.
    pub fn basic_auth(self, username: impl Into<String>, password: impl Into<String>) -> Self {
        self.auth(Auth::basic(username, password))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_builder() {
        let req = Request::get("/api/users")
            .header("X-Custom", "value")
            .query_param("limit", "10")
            .bearer_token("secret-token");

        assert_eq!(req.method, Method::GET);
        assert_eq!(req.path, "/api/users");
        assert_eq!(req.headers.get("X-Custom"), Some(&"value".to_string()));
        assert_eq!(
            req.query.as_ref().unwrap().get("limit"),
            Some(&"10".to_string())
        );
        assert!(matches!(req.auth, Some(Auth::Bearer(_))));
    }

    #[test]
    fn test_request_body_json() {
        let body = RequestBody::json(&serde_json::json!({"key": "value"})).unwrap();
        assert!(matches!(body, RequestBody::Json(_)));
    }
}
