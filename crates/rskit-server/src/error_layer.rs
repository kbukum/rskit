//! Tower layer that intercepts gRPC responses and enriches error statuses
//! with structured [`AppError`] details.
//!
//! Applied automatically by [`GrpcServerBuilder`] so service implementations
//! can simply return `Status::from(AppError)` (or let the `?` operator do it)
//! and the canonical RFC 9457 JSON error body is always present in the status details.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use http::{Request, Response};
use tonic::body::Body;
use tower::{Layer, Service};

use rskit_errors::{AppError, ProblemDetail};

// ── Layer ────────────────────────────────────────────────────────────────────

/// A [`Layer`] that wraps each gRPC service to ensure every error
/// `tonic::Status` carries structured RFC 9457 JSON details.
#[derive(Debug, Clone, Default)]
pub struct ErrorLayer;

impl ErrorLayer {
    /// Create a new `ErrorLayer`.
    pub fn new() -> Self {
        Self
    }
}

impl<S> Layer<S> for ErrorLayer {
    type Service = ErrorService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ErrorService { inner }
    }
}

// ── Service ──────────────────────────────────────────────────────────────────

/// The [`Service`] produced by [`ErrorLayer`].
///
/// It forwards requests to the inner service and, if the response indicates a
/// gRPC error (via the `grpc-status` header) without structured details, it
/// enriches the trailers with canonical RFC 9457 error JSON.
#[derive(Debug, Clone)]
pub struct ErrorService<S> {
    inner: S,
}

impl<S: tonic::server::NamedService> tonic::server::NamedService for ErrorService<S> {
    const NAME: &'static str = S::NAME;
}

impl<S> Service<Request<Body>> for ErrorService<S>
where
    S: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let mut inner = self.inner.clone();
        Box::pin(async move {
            let response = inner.call(req).await?;
            Ok(enrich_error_response(response))
        })
    }
}

/// If the response carries a `grpc-status` header indicating an error but no
/// structured details in `grpc-status-details-bin`, synthesise them from the
/// status code and message.
fn enrich_error_response(response: Response<Body>) -> Response<Body> {
    // Only process error responses (grpc-status != 0)
    let status_code = response
        .headers()
        .get("grpc-status")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(0);

    if status_code == 0 {
        return response;
    }

    // If details are already present, leave the response as-is — the service
    // (or `From<AppError> for tonic::Status`) already attached them.
    if response.headers().contains_key("grpc-status-details-bin") {
        return response;
    }

    // Synthesise RFC 9457 ProblemDetail from the raw code + message.
    let grpc_message = response
        .headers()
        .get("grpc-message")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown error")
        .to_string();

    let error_code = grpc_code_to_error_code(status_code);
    let app_err = AppError::new(error_code, grpc_message);
    let problem = ProblemDetail::from(&app_err);

    if let Ok(json_bytes) = serde_json::to_vec(&problem) {
        let encoded = base64_encode(&json_bytes);
        let (mut parts, body) = response.into_parts();
        if let Ok(val) = http::HeaderValue::from_str(&encoded) {
            parts.headers.insert("grpc-status-details-bin", val);
        }
        Response::from_parts(parts, body)
    } else {
        response
    }
}

fn grpc_code_to_error_code(code: i32) -> rskit_errors::ErrorCode {
    use rskit_errors::ErrorCode;
    match code {
        1 => ErrorCode::Internal,     // CANCELLED
        2 => ErrorCode::Internal,     // UNKNOWN
        3 => ErrorCode::InvalidInput, // INVALID_ARGUMENT
        4 => ErrorCode::Timeout,      // DEADLINE_EXCEEDED
        5 => ErrorCode::NotFound,
        6 => ErrorCode::AlreadyExists,
        7 => ErrorCode::Forbidden,           // PERMISSION_DENIED
        8 => ErrorCode::RateLimited,         // RESOURCE_EXHAUSTED
        9 => ErrorCode::Conflict,            // FAILED_PRECONDITION
        10 => ErrorCode::Conflict,           // ABORTED
        11 => ErrorCode::InvalidInput,       // OUT_OF_RANGE
        12 => ErrorCode::Internal,           // UNIMPLEMENTED
        13 => ErrorCode::Internal,           // INTERNAL
        14 => ErrorCode::ServiceUnavailable, // UNAVAILABLE
        15 => ErrorCode::Internal,           // DATA_LOSS
        16 => ErrorCode::Unauthorized,       // UNAUTHENTICATED
        _ => ErrorCode::Internal,
    }
}

/// Base64-encode bytes (standard base64, no padding trimming).
fn base64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_errors::ErrorCode;

    #[test]
    fn grpc_code_mapping_covers_all_codes() {
        assert_eq!(grpc_code_to_error_code(3), ErrorCode::InvalidInput);
        assert_eq!(grpc_code_to_error_code(4), ErrorCode::Timeout);
        assert_eq!(grpc_code_to_error_code(5), ErrorCode::NotFound);
        assert_eq!(grpc_code_to_error_code(6), ErrorCode::AlreadyExists);
        assert_eq!(grpc_code_to_error_code(7), ErrorCode::Forbidden);
        assert_eq!(grpc_code_to_error_code(8), ErrorCode::RateLimited);
        assert_eq!(grpc_code_to_error_code(14), ErrorCode::ServiceUnavailable);
        assert_eq!(grpc_code_to_error_code(16), ErrorCode::Unauthorized);
        assert_eq!(grpc_code_to_error_code(99), ErrorCode::Internal);
    }

    #[test]
    fn enrich_skips_ok_response() {
        let resp = Response::builder()
            .header("grpc-status", "0")
            .body(Body::default())
            .unwrap();
        let result = enrich_error_response(resp);
        assert!(!result.headers().contains_key("grpc-status-details-bin"));
    }

    #[test]
    fn enrich_skips_when_details_already_present() {
        let resp = Response::builder()
            .header("grpc-status", "5")
            .header("grpc-status-details-bin", "existing")
            .body(Body::default())
            .unwrap();
        let result = enrich_error_response(resp);
        assert_eq!(
            result.headers().get("grpc-status-details-bin").unwrap(),
            "existing"
        );
    }

    #[test]
    fn enrich_adds_problem_detail_for_error_without_details() {
        let resp = Response::builder()
            .header("grpc-status", "5")
            .header("grpc-message", "user not found")
            .body(Body::default())
            .unwrap();
        let result = enrich_error_response(resp);
        assert!(result.headers().contains_key("grpc-status-details-bin"));

        let encoded = result
            .headers()
            .get("grpc-status-details-bin")
            .unwrap()
            .to_str()
            .unwrap();
        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap();
        let pd: ProblemDetail = serde_json::from_slice(&decoded).unwrap();
        assert_eq!(pd.code, ErrorCode::NotFound);
        assert_eq!(pd.detail, "user not found");
        assert_eq!(pd.status, 404);
        assert!(pd.error_type.ends_with("not-found"));
    }

    #[test]
    fn enrich_handles_missing_grpc_status_header() {
        let resp = Response::builder().body(Body::default()).unwrap();
        let result = enrich_error_response(resp);
        assert!(!result.headers().contains_key("grpc-status-details-bin"));
    }

    #[test]
    fn error_layer_default_and_new_are_equivalent() {
        let _l1 = ErrorLayer;
        let _l2 = ErrorLayer::new();
    }
}
