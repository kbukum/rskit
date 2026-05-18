//! HTTP security header policy shared by transport adapters.

use http::{
    HeaderName, HeaderValue,
    header::{
        CONTENT_SECURITY_POLICY, REFERRER_POLICY, STRICT_TRANSPORT_SECURITY,
        X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
    },
};
use rskit_errors::{AppError, AppResult};

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

    /// Build validated header pairs for transport adapters.
    ///
    /// # Errors
    /// Returns an error when a configured header value is invalid.
    pub fn header_pairs(&self) -> AppResult<Vec<(HeaderName, HeaderValue)>> {
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
