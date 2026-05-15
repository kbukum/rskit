//! Security headers and transport security policy.

#![warn(missing_docs)]

use std::sync::Arc;
use std::task::{Context, Poll};

use http::{
    HeaderName, HeaderValue, Method, Request, Response,
    header::{
        CONTENT_SECURITY_POLICY, REFERRER_POLICY, STRICT_TRANSPORT_SECURITY,
        X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
    },
};
use rskit_errors::{AppError, AppResult};
use rskit_validation::Validator;
use tower::{Layer, Service};
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};

const HSTS_HEADER_VALUE: &str = "max-age=63072000; includeSubDomains; preload";
const CSP_HEADER_VALUE: &str =
    "default-src 'self'; base-uri 'self'; frame-ancestors 'none'; object-src 'none'";
const PERMISSIONS_POLICY: HeaderName = HeaderName::from_static("permissions-policy");
const REFERRER_POLICY_VALUE: &str = "strict-origin-when-cross-origin";
const PERMISSIONS_POLICY_VALUE: &str = "accelerometer=(), camera=(), geolocation=(), microphone=()";
const X_CONTENT_TYPE_OPTIONS_VALUE: &str = "nosniff";
const X_FRAME_OPTIONS_VALUE: &str = "DENY";

/// Transport security mode used when applying response headers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum TransportSecurity {
    /// Production mode: HTTPS is required and HSTS is emitted.
    #[default]
    HttpsOnly,
    /// Local/insecure mode: HSTS is omitted while the remaining headers still apply.
    AllowInsecureLocal,
}

/// Secure-by-default response header configuration.
#[derive(Debug, Clone)]
pub struct SecurityHeadersConfig {
    transport_security: TransportSecurity,
    content_security_policy: Option<String>,
    permissions_policy: Option<String>,
    referrer_policy: Option<String>,
    frame_options: Option<String>,
    content_type_options: Option<String>,
}

impl Default for SecurityHeadersConfig {
    fn default() -> Self {
        Self {
            transport_security: TransportSecurity::HttpsOnly,
            content_security_policy: Some(CSP_HEADER_VALUE.to_string()),
            permissions_policy: Some(PERMISSIONS_POLICY_VALUE.to_string()),
            referrer_policy: Some(REFERRER_POLICY_VALUE.to_string()),
            frame_options: Some(X_FRAME_OPTIONS_VALUE.to_string()),
            content_type_options: Some(X_CONTENT_TYPE_OPTIONS_VALUE.to_string()),
        }
    }
}

impl SecurityHeadersConfig {
    /// Validate the configuration.
    ///
    /// # Errors
    /// Returns an error when the configuration cannot be safely applied.
    pub fn validate(&self) -> AppResult<()> {
        if matches!(
            self.transport_security,
            TransportSecurity::AllowInsecureLocal
        ) && self.content_security_policy.is_none()
            && self.permissions_policy.is_none()
            && self.referrer_policy.is_none()
            && self.frame_options.is_none()
            && self.content_type_options.is_none()
        {
            return Err(AppError::invalid_input(
                "security_headers",
                "at least one security header must remain enabled",
            ));
        }

        Ok(())
    }

    /// Select the transport security mode.
    #[must_use]
    pub const fn with_transport_security(mut self, transport_security: TransportSecurity) -> Self {
        self.transport_security = transport_security;
        self
    }

    /// Override the Content Security Policy header. Pass `None` to disable it.
    #[must_use]
    pub fn with_content_security_policy(mut self, value: Option<String>) -> Self {
        self.content_security_policy = value;
        self
    }

    /// Override the Permissions Policy header. Pass `None` to disable it.
    #[must_use]
    pub fn with_permissions_policy(mut self, value: Option<String>) -> Self {
        self.permissions_policy = value;
        self
    }

    /// Override the Referrer Policy header. Pass `None` to disable it.
    #[must_use]
    pub fn with_referrer_policy(mut self, value: Option<String>) -> Self {
        self.referrer_policy = value;
        self
    }

    /// Override the X-Frame-Options header. Pass `None` to disable it.
    #[must_use]
    pub fn with_frame_options(mut self, value: Option<String>) -> Self {
        self.frame_options = value;
        self
    }

    /// Override the X-Content-Type-Options header. Pass `None` to disable it.
    #[must_use]
    pub fn with_content_type_options(mut self, value: Option<String>) -> Self {
        self.content_type_options = value;
        self
    }

    fn header_pairs(&self) -> AppResult<Vec<(HeaderName, HeaderValue)>> {
        self.validate()?;

        let mut headers = Vec::new();
        if matches!(self.transport_security, TransportSecurity::HttpsOnly) {
            headers.push((
                STRICT_TRANSPORT_SECURITY,
                HeaderValue::from_static(HSTS_HEADER_VALUE),
            ));
        }
        if let Some(value) = &self.content_security_policy {
            headers.push((
                CONTENT_SECURITY_POLICY,
                header_value(&CONTENT_SECURITY_POLICY, value)?,
            ));
        }
        if let Some(value) = &self.permissions_policy {
            headers.push((
                PERMISSIONS_POLICY,
                header_value(&PERMISSIONS_POLICY, value)?,
            ));
        }
        if let Some(value) = &self.referrer_policy {
            headers.push((REFERRER_POLICY, header_value(&REFERRER_POLICY, value)?));
        }
        if let Some(value) = &self.frame_options {
            headers.push((X_FRAME_OPTIONS, header_value(&X_FRAME_OPTIONS, value)?));
        }
        if let Some(value) = &self.content_type_options {
            headers.push((
                X_CONTENT_TYPE_OPTIONS,
                header_value(&X_CONTENT_TYPE_OPTIONS, value)?,
            ));
        }
        Ok(headers)
    }
}

fn header_value(header: &HeaderName, value: &str) -> AppResult<HeaderValue> {
    HeaderValue::from_str(value).map_err(|error| {
        AppError::invalid_input(header.as_str(), format!("invalid header value: {error}"))
    })
}

/// Tower layer that applies secure response headers.
///
/// Headers are validated and precomputed at construction time so that the
/// per-request path is infallible and the layer fails fast on invalid config.
#[derive(Debug, Clone)]
pub struct SecurityHeadersLayer {
    headers: Arc<Vec<(HeaderName, HeaderValue)>>,
}

impl SecurityHeadersLayer {
    /// Create a new layer from validated configuration.
    ///
    /// Headers are precomputed here; any invalid configuration is rejected at
    /// this point rather than silently falling back at request time.
    ///
    /// # Errors
    /// Returns an error when the configuration is invalid or a header value
    /// cannot be constructed.
    pub fn new(config: &SecurityHeadersConfig) -> AppResult<Self> {
        let headers = Arc::new(config.header_pairs()?);
        Ok(Self { headers })
    }
}

impl<S> Layer<S> for SecurityHeadersLayer {
    type Service = SecurityHeadersService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        SecurityHeadersService {
            inner,
            headers: Arc::clone(&self.headers),
        }
    }
}

/// Service produced by [`SecurityHeadersLayer`].
#[derive(Debug, Clone)]
pub struct SecurityHeadersService<S> {
    inner: S,
    headers: Arc<Vec<(HeaderName, HeaderValue)>>,
}

impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for SecurityHeadersService<S>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
    ResBody: Send + 'static,
{
    type Response = Response<ResBody>;
    type Error = S::Error;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        // Use mem::replace to honour the Tower poll_ready → call contract:
        // the instance that was polled ready (now in `inner`) handles this
        // request; self.inner receives a fresh clone for the next cycle.
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);
        let headers = Arc::clone(&self.headers);
        Box::pin(async move {
            let mut response = inner.call(req).await?;
            let response_headers = response.headers_mut();
            for (name, value) in headers.iter() {
                response_headers
                    .entry(name)
                    .or_insert_with(|| value.clone());
            }
            Ok(response)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::{StatusCode, header::HeaderValue};
    use tower::{ServiceBuilder, ServiceExt, service_fn};

    #[tokio::test]
    async fn defaults_apply_secure_headers() {
        let service = ServiceBuilder::new()
            .layer(SecurityHeadersLayer::new(&SecurityHeadersConfig::default()).unwrap())
            .service(service_fn(|_req: Request<()>| async {
                Ok::<_, std::convert::Infallible>(
                    Response::builder().status(StatusCode::OK).body(()).unwrap(),
                )
            }));

        let response = service.oneshot(Request::new(())).await.unwrap();
        let headers = response.headers();

        assert_eq!(
            headers.get(STRICT_TRANSPORT_SECURITY),
            Some(&HeaderValue::from_static(HSTS_HEADER_VALUE))
        );
        assert_eq!(
            headers.get(X_CONTENT_TYPE_OPTIONS),
            Some(&HeaderValue::from_static(X_CONTENT_TYPE_OPTIONS_VALUE))
        );
        assert_eq!(
            headers.get(X_FRAME_OPTIONS),
            Some(&HeaderValue::from_static(X_FRAME_OPTIONS_VALUE))
        );
    }

    #[tokio::test]
    async fn insecure_local_mode_omits_hsts() {
        let service = ServiceBuilder::new()
            .layer(
                SecurityHeadersLayer::new(
                    &SecurityHeadersConfig::default()
                        .with_transport_security(TransportSecurity::AllowInsecureLocal),
                )
                .unwrap(),
            )
            .service(service_fn(|_req: Request<()>| async {
                Ok::<_, std::convert::Infallible>(
                    Response::builder().status(StatusCode::OK).body(()).unwrap(),
                )
            }));

        let response = service.oneshot(Request::new(())).await.unwrap();
        assert!(response.headers().get(STRICT_TRANSPORT_SECURITY).is_none());
        assert!(response.headers().get(CONTENT_SECURITY_POLICY).is_some());
    }

    #[tokio::test]
    async fn existing_headers_are_not_overwritten() {
        let service = ServiceBuilder::new()
            .layer(SecurityHeadersLayer::new(&SecurityHeadersConfig::default()).unwrap())
            .service(service_fn(|_req: Request<()>| async {
                let mut response = Response::builder().status(StatusCode::OK).body(()).unwrap();
                response
                    .headers_mut()
                    .insert(REFERRER_POLICY, HeaderValue::from_static("same-origin"));
                Ok::<_, std::convert::Infallible>(response)
            }));

        let response = service.oneshot(Request::new(())).await.unwrap();
        assert_eq!(
            response.headers().get(REFERRER_POLICY),
            Some(&HeaderValue::from_static("same-origin"))
        );
    }

    #[test]
    fn all_headers_disabled_with_allow_insecure_local_is_rejected() {
        let config = SecurityHeadersConfig::default()
            .with_transport_security(TransportSecurity::AllowInsecureLocal)
            .with_content_security_policy(None)
            .with_permissions_policy(None)
            .with_referrer_policy(None)
            .with_frame_options(None)
            .with_content_type_options(None);
        assert!(
            SecurityHeadersLayer::new(&config).is_err(),
            "AllowInsecureLocal with all headers disabled must be rejected"
        );
    }

    #[test]
    fn cors_defaults_are_deny_by_default() {
        let config = CorsConfig::default();
        assert!(config.allowed_origins.is_empty());
        assert!(config.layer().is_ok());
    }

    #[test]
    fn cors_rejects_wildcard_origin() {
        let config = CorsConfig {
            allowed_origins: vec!["*".to_string()],
            allowed_methods: vec!["GET".to_string()],
            allowed_headers: vec!["authorization".to_string()],
            allow_credentials: false,
            max_age: std::time::Duration::from_mins(1),
        };
        assert!(config.layer().is_err());
    }

    #[test]
    fn path_validation_rejects_traversal_and_mixed_separators() {
        assert!(validate_safe_path("tenant/report.json").is_ok());
        assert!(validate_safe_path("../secret").is_err());
        assert!(validate_safe_path("tenant\\..\\secret").is_err());
    }

    #[test]
    fn unicode_hardening_rejects_rtl_override_and_selected_confusables() {
        assert!(reject_dangerous_unicode("identifier", "safe-id").is_ok());
        assert!(reject_dangerous_unicode("identifier", "safe\u{202e}txt").is_err());
        assert!(reject_dangerous_unicode("identifier", "раypal").is_err());
        assert!(reject_dangerous_unicode("identifier", "павел").is_ok());
    }
}

/// Cross-origin resource sharing configuration.
///
/// The default is deny-by-default: no origins, methods, headers, or credentials
/// are allowed unless explicitly configured.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct CorsConfig {
    /// Allowed Origin header values.
    pub allowed_origins: Vec<String>,
    /// Allowed HTTP methods.
    pub allowed_methods: Vec<String>,
    /// Allowed request headers.
    pub allowed_headers: Vec<String>,
    /// Whether to allow credentials.
    pub allow_credentials: bool,
    /// Cache duration for pre-flight responses.
    pub max_age: std::time::Duration,
}

impl CorsConfig {
    /// Validate the CORS policy.
    ///
    /// # Errors
    /// Returns an error when an origin, method, or header is invalid or when
    /// wildcard origins are combined with credentials.
    pub fn validate(&self) -> AppResult<()> {
        for origin in &self.allowed_origins {
            Validator::new().url("allowed_origins", origin).validate()?;
            if origin == "*" {
                return Err(AppError::invalid_input(
                    "allowed_origins",
                    "wildcard origins are not allowed",
                ));
            }
        }
        if self.allow_credentials && self.allowed_origins.is_empty() {
            return Err(AppError::invalid_input(
                "allowed_origins",
                "credentials require an explicit origin allow-list",
            ));
        }
        Ok(())
    }

    /// Build a Tower CORS layer from this explicit policy.
    ///
    /// # Errors
    /// Returns an error when any configured origin, method, or header is invalid.
    pub fn layer(&self) -> AppResult<CorsLayer> {
        self.validate()?;

        let origins = self
            .allowed_origins
            .iter()
            .map(|origin| header_value(&http::header::ORIGIN, origin))
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

/// Validate that a path-like input cannot traverse outside its base.
pub fn validate_safe_path(path: &str) -> AppResult<()> {
    reject_dangerous_unicode("path", path)?;
    if path.is_empty()
        || path.starts_with('/')
        || path.starts_with('\\')
        || path.split(['/', '\\']).any(|segment| {
            segment.is_empty() || segment == "." || segment == ".." || segment.contains(':')
        })
    {
        return Err(AppError::invalid_input(
            "path",
            "path must be relative and must not contain traversal segments",
        ));
    }
    Ok(())
}

/// Reject Unicode controls and selected confusable/security-sensitive code points.
pub fn reject_dangerous_unicode(field: &str, value: &str) -> AppResult<()> {
    let contains_ascii_latin = value.chars().any(|ch| ch.is_ascii_alphabetic());
    for ch in value.chars() {
        if ch.is_control()
            || matches!(
                ch,
                '\u{202A}'..='\u{202E}'
                    | '\u{2066}'..='\u{2069}'
                    | '\u{200E}'
                    | '\u{200F}'
            )
            || (contains_ascii_latin && is_common_latin_confusable(ch))
        {
            return Err(AppError::invalid_input(
                field,
                "input contains forbidden Unicode control or confusable characters",
            ));
        }
    }
    Ok(())
}

const fn is_common_latin_confusable(ch: char) -> bool {
    matches!(
        ch,
        // Common Cyrillic and Greek lookalikes used in mixed-script spoofing.
        'А' | 'В'
            | 'Е'
            | 'К'
            | 'М'
            | 'Н'
            | 'О'
            | 'Р'
            | 'С'
            | 'Т'
            | 'Х'
            | 'а'
            | 'е'
            | 'о'
            | 'р'
            | 'с'
            | 'у'
            | 'х'
            | 'Α'
            | 'Β'
            | 'Ε'
            | 'Ζ'
            | 'Η'
            | 'Ι'
            | 'Κ'
            | 'Μ'
            | 'Ν'
            | 'Ο'
            | 'Ρ'
            | 'Τ'
            | 'Υ'
            | 'Χ'
            | 'α'
            | 'ο'
            | 'ρ'
    )
}
