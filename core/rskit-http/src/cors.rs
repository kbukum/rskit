//! CORS policy and Tower layer construction.

use std::time::Duration;

use http::{HeaderName, HeaderValue, Method};
use rskit_errors::{AppError, AppResult};
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};

/// Cross-origin resource sharing policy.
///
/// The default is deny-by-default: no origins, methods, headers, or credentials
/// are allowed unless explicitly configured.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct CorsPolicy {
    /// Allowed Origin header values.
    pub allowed_origins: Vec<String>,
    /// Allowed HTTP methods.
    pub allowed_methods: Vec<String>,
    /// Allowed request headers.
    pub allowed_headers: Vec<String>,
    /// Whether to allow credentials.
    pub allow_credentials: bool,
    /// Cache duration for pre-flight responses.
    pub max_age: Duration,
}

impl CorsPolicy {
    /// Validate the CORS policy.
    ///
    /// # Errors
    /// Returns an error when an origin, method, or header is invalid.
    pub fn validate(&self) -> AppResult<()> {
        for origin in &self.allowed_origins {
            validate_allowed_origin(origin)?;
        }
        if self.allow_credentials && self.allowed_origins.is_empty() {
            return Err(AppError::invalid_input(
                "allowed_origins",
                "credentials require an explicit origin allow-list",
            ));
        }

        for method in &self.allowed_methods {
            method.parse::<Method>().map_err(|error| {
                AppError::invalid_input("allowed_methods", format!("invalid method: {error}"))
            })?;
        }
        for header in &self.allowed_headers {
            HeaderName::from_bytes(header.as_bytes()).map_err(|error| {
                AppError::invalid_input("allowed_headers", format!("invalid header: {error}"))
            })?;
        }
        Ok(())
    }

    /// Build a Tower CORS layer from this policy.
    ///
    /// # Errors
    /// Returns an error when an origin, method, or header is invalid.
    pub fn layer(&self) -> AppResult<CorsLayer> {
        self.validate()?;

        let origins = self
            .allowed_origins
            .iter()
            .map(|origin| {
                HeaderValue::from_str(origin).map_err(|error| {
                    AppError::invalid_input("allowed_origins", format!("invalid origin: {error}"))
                })
            })
            .collect::<AppResult<Vec<_>>>()?;
        let methods = self
            .allowed_methods
            .iter()
            .map(|method| {
                method.parse::<Method>().map_err(|error| {
                    AppError::invalid_input("allowed_methods", format!("invalid method: {error}"))
                })
            })
            .collect::<AppResult<Vec<_>>>()?;
        let headers = self
            .allowed_headers
            .iter()
            .map(|header| {
                HeaderName::from_bytes(header.as_bytes()).map_err(|error| {
                    AppError::invalid_input("allowed_headers", format!("invalid header: {error}"))
                })
            })
            .collect::<AppResult<Vec<_>>>()?;

        let mut layer = CorsLayer::new()
            .allow_origin(AllowOrigin::list(origins))
            .allow_methods(AllowMethods::list(methods))
            .allow_headers(AllowHeaders::list(headers))
            .allow_credentials(self.allow_credentials);

        if !self.max_age.is_zero() {
            layer = layer.max_age(self.max_age);
        }

        Ok(layer)
    }
}

fn validate_allowed_origin(origin: &str) -> AppResult<()> {
    if origin == "*" {
        return Err(AppError::invalid_input(
            "allowed_origins",
            "wildcard origins are not allowed",
        ));
    }

    let parsed = origin.parse::<http::Uri>().map_err(|error| {
        AppError::invalid_input("allowed_origins", format!("invalid origin: {error}"))
    })?;

    match parsed.scheme_str() {
        Some("http" | "https") => {}
        scheme => {
            return Err(AppError::invalid_input(
                "allowed_origins",
                format!("origin scheme must be http or https, got {scheme:?}"),
            ));
        }
    }

    let Some(authority) = parsed.authority() else {
        return Err(AppError::invalid_input(
            "allowed_origins",
            "origin must include a host",
        ));
    };
    if authority.as_str().contains('@') {
        return Err(AppError::invalid_input(
            "allowed_origins",
            "origin must not contain credentials",
        ));
    }

    let path = parsed.path();
    if path != "/" && !path.is_empty() {
        return Err(AppError::invalid_input(
            "allowed_origins",
            "origin must not contain a path",
        ));
    }
    if parsed.query().is_some() {
        return Err(AppError::invalid_input(
            "allowed_origins",
            "origin must not contain a query",
        ));
    }
    if origin.contains('#') {
        return Err(AppError::invalid_input(
            "allowed_origins",
            "origin must not contain a fragment",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_deny_by_default() {
        let policy = CorsPolicy::default();
        assert!(policy.allowed_origins.is_empty());
        assert!(policy.allowed_methods.is_empty());
        assert!(policy.allowed_headers.is_empty());
        assert!(policy.validate().is_ok());
    }

    #[test]
    fn rejects_wildcard_origin_before_url_validation() {
        let policy = CorsPolicy {
            allowed_origins: vec!["*".to_string()],
            allowed_methods: vec!["GET".to_string()],
            allowed_headers: vec!["authorization".to_string()],
            allow_credentials: false,
            max_age: Duration::from_mins(1),
        };
        let err = policy.validate().unwrap_err();
        assert!(err.to_string().contains("wildcard origins are not allowed"));
    }

    #[test]
    fn rejects_invalid_methods_and_headers() {
        let policy = CorsPolicy {
            allowed_origins: vec!["https://example.com".to_string()],
            allowed_methods: vec!["bad method".to_string()],
            allowed_headers: vec!["authorization".to_string()],
            allow_credentials: false,
            max_age: Duration::from_mins(1),
        };
        assert!(policy.validate().is_err());

        let policy = CorsPolicy {
            allowed_origins: vec!["https://example.com".to_string()],
            allowed_methods: vec!["GET".to_string()],
            allowed_headers: vec!["bad header".to_string()],
            allow_credentials: false,
            max_age: Duration::from_mins(1),
        };
        assert!(policy.validate().is_err());
    }
}
