use http::{Extensions, HeaderMap};

/// Request identifier carried through HTTP request extensions.
#[derive(Debug, Clone)]
pub struct RequestId(pub String);

impl RequestId {
    /// Read a request id from headers or generate a new UUID.
    #[must_use]
    pub fn from_headers(headers: &HeaderMap) -> Self {
        Self(
            headers
                .get("x-request-id")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        )
    }
}

/// Correlation identifier carried through HTTP request extensions.
#[derive(Debug, Clone)]
pub struct CorrelationId(pub String);

impl CorrelationId {
    /// Read a correlation id from headers or generate a new UUID.
    #[must_use]
    pub fn from_headers(headers: &HeaderMap) -> Self {
        Self(
            headers
                .get("x-correlation-id")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        )
    }
}

/// Store a [`RequestId`] in request extensions.
pub fn set_request_id(extensions: &mut Extensions, request_id: impl Into<String>) {
    extensions.insert(RequestId(request_id.into()));
}

/// Retrieve a [`RequestId`] from request extensions.
#[must_use]
pub fn request_id_from_extensions(extensions: &Extensions) -> Option<&RequestId> {
    extensions.get::<RequestId>()
}

/// Store a [`CorrelationId`] in request extensions.
pub fn set_correlation_id(extensions: &mut Extensions, correlation_id: impl Into<String>) {
    extensions.insert(CorrelationId(correlation_id.into()));
}

/// Retrieve a [`CorrelationId`] from request extensions.
#[must_use]
pub fn correlation_id_from_extensions(extensions: &Extensions) -> Option<&CorrelationId> {
    extensions.get::<CorrelationId>()
}
