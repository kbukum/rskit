use async_trait::async_trait;
use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
};

/// Axum extractor that reads the `X-Request-Id` header.
#[derive(Debug, Clone)]
pub struct RequestId(pub String);

#[async_trait]
impl<S: Send + Sync> FromRequestParts<S> for RequestId {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let id = parts
            .headers
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_owned())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        Ok(RequestId(id))
    }
}

/// Axum extractor that reads the `X-Correlation-Id` header.
#[derive(Debug, Clone)]
pub struct CorrelationId(pub String);

#[async_trait]
impl<S: Send + Sync> FromRequestParts<S> for CorrelationId {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let id = parts
            .headers
            .get("x-correlation-id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_owned())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        Ok(CorrelationId(id))
    }
}
