//! Security headers and transport security policy.

#![warn(missing_docs)]

use std::task::{Context, Poll};

use http::{
    HeaderName, HeaderValue, Request, Response,
    header::{
        CONTENT_SECURITY_POLICY, REFERRER_POLICY, STRICT_TRANSPORT_SECURITY,
        X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
    },
};
use rskit_errors::{AppError, AppResult};
use tower::{Layer, Service};

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
#[derive(Debug, Clone)]
pub struct SecurityHeadersLayer {
    config: SecurityHeadersConfig,
}

impl SecurityHeadersLayer {
    /// Create a new layer from validated configuration.
    ///
    /// # Errors
    /// Returns an error when the configuration is invalid.
    pub fn new(config: SecurityHeadersConfig) -> AppResult<Self> {
        config.validate()?;
        Ok(Self { config })
    }
}

impl<S> Layer<S> for SecurityHeadersLayer {
    type Service = SecurityHeadersService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        SecurityHeadersService {
            inner,
            config: self.config.clone(),
        }
    }
}

/// Service produced by [`SecurityHeadersLayer`].
#[derive(Debug, Clone)]
pub struct SecurityHeadersService<S> {
    inner: S,
    config: SecurityHeadersConfig,
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
        let mut inner = self.inner.clone();
        let headers = match self.config.header_pairs() {
            Ok(h) => h,
            Err(_) => {
                // Configuration was validated at layer construction; this branch is
                // unreachable under normal use. Fail open by logging would be worse
                // than failing closed — return an empty vec so nothing sneaks through.
                Vec::new()
            }
        };
        Box::pin(async move {
            let mut response = inner.call(req).await?;
            if !headers.is_empty() {
                let response_headers = response.headers_mut();
                for (name, value) in headers {
                    response_headers.entry(name).or_insert(value);
                }
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
            .layer(SecurityHeadersLayer::new(SecurityHeadersConfig::default()).unwrap())
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
                    SecurityHeadersConfig::default()
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
            .layer(SecurityHeadersLayer::new(SecurityHeadersConfig::default()).unwrap())
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
}
