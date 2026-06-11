#![allow(missing_docs)]

use std::time::Duration;

use http::{Extensions, HeaderMap, HeaderValue, Method, Request, Response, StatusCode, header};
use rskit_errors::{AppError, ErrorCode};
use rskit_http::{
    CorrelationId, CorsPolicy, HttpRequest, HttpResponse, RequestId, SecurityHeadersConfig,
    SecurityHeadersLayer, TenantConfig, TransportSecurity, app_error_status,
    correlation_id_from_extensions, is_success_status, request_id_from_extensions,
    set_correlation_id, set_request_id, set_tenant_in_extensions, status_to_error_code,
    tenant_from_extensions,
};
use tower::{Layer, ServiceExt, service_fn};

#[test]
fn request_and_correlation_ids_prefer_valid_headers_and_fallback_to_uuid() {
    let mut headers = HeaderMap::new();
    headers.insert("x-request-id", HeaderValue::from_static("req-123"));
    headers.insert("x-correlation-id", HeaderValue::from_static("corr-456"));
    assert_eq!(RequestId::from_headers(&headers).0, "req-123");
    assert_eq!(CorrelationId::from_headers(&headers).0, "corr-456");

    let generated = RequestId::from_headers(&HeaderMap::new()).0;
    assert_eq!(generated.len(), 36);

    let mut extensions = Extensions::new();
    set_request_id(&mut extensions, "req-ext");
    set_correlation_id(&mut extensions, "corr-ext");
    assert_eq!(
        request_id_from_extensions(&extensions).unwrap().0,
        "req-ext"
    );
    assert_eq!(
        correlation_id_from_extensions(&extensions).unwrap().0,
        "corr-ext"
    );
}

#[test]
fn tenant_config_reads_headers_fallbacks_and_extensions() {
    let mut headers = HeaderMap::new();
    headers.insert("x-org", HeaderValue::from_static("org-1"));
    let config = TenantConfig {
        header_name: "x-org".to_owned(),
        required: true,
        fallback: Some("fallback".to_owned()),
    };
    assert!(config.is_required());
    assert_eq!(config.tenant_from_headers(&headers).unwrap().0, "org-1");
    assert_eq!(
        config.tenant_from_headers(&HeaderMap::new()).unwrap().0,
        "fallback"
    );

    let mut extensions = Extensions::new();
    set_tenant_in_extensions(&mut extensions, "tenant-ext");
    assert_eq!(tenant_from_extensions(&extensions).unwrap().0, "tenant-ext");
}

#[test]
fn cors_policy_accepts_explicit_origins_and_rejects_unsafe_shapes() {
    let valid = CorsPolicy {
        allowed_origins: vec!["https://app.example.com".to_owned()],
        allowed_methods: vec![
            Method::GET.as_str().to_owned(),
            Method::POST.as_str().to_owned(),
        ],
        allowed_headers: vec!["authorization".to_owned(), "content-type".to_owned()],
        allow_credentials: true,
        max_age: Duration::from_secs(600),
    };
    valid.validate().unwrap();
    let _ = valid.layer().unwrap();

    for origin in [
        "*",
        "ftp://example.com",
        "https://user@example.com",
        "https://example.com/path",
        "https://example.com?x=1",
        "https://example.com#frag",
    ] {
        let policy = CorsPolicy {
            allowed_origins: vec![origin.to_owned()],
            ..valid.clone()
        };
        assert!(
            policy.validate().is_err(),
            "origin {origin} should be rejected"
        );
    }

    let credentials_without_origins = CorsPolicy {
        allow_credentials: true,
        ..CorsPolicy::default()
    };
    assert!(credentials_without_origins.validate().is_err());
}

#[test]
fn status_helpers_cover_success_error_and_fallback_mappings() {
    assert!(is_success_status(StatusCode::NO_CONTENT));
    assert_eq!(
        status_to_error_code(StatusCode::UNAUTHORIZED),
        ErrorCode::Unauthorized
    );
    assert_eq!(
        status_to_error_code(StatusCode::FORBIDDEN),
        ErrorCode::Forbidden
    );
    assert_eq!(
        status_to_error_code(StatusCode::CONFLICT),
        ErrorCode::Conflict
    );
    assert_eq!(
        status_to_error_code(StatusCode::REQUEST_TIMEOUT),
        ErrorCode::Cancelled
    );
    assert_eq!(
        status_to_error_code(StatusCode::IM_A_TEAPOT),
        ErrorCode::ExternalService
    );
    assert_eq!(status_to_error_code(StatusCode::OK), ErrorCode::Internal);

    let error = AppError::new(ErrorCode::RateLimited, "slow down");
    assert_eq!(app_error_status(&error), StatusCode::TOO_MANY_REQUESTS);

    let request: HttpRequest = Request::builder().uri("/ready").body(Vec::new()).unwrap();
    let response: HttpResponse = Response::builder()
        .status(StatusCode::ACCEPTED)
        .body(Vec::new())
        .unwrap();
    assert_eq!(request.uri(), "/ready");
    assert_eq!(response.status(), StatusCode::ACCEPTED);
}

#[tokio::test]
async fn security_headers_layer_inserts_defaults_without_overwriting_existing_values() {
    let layer = SecurityHeadersLayer::new(
        &SecurityHeadersConfig::default().with_transport_security(TransportSecurity::HttpsOnly),
    )
    .unwrap();
    let service = layer.layer(service_fn(|_req: Request<()>| async {
        let mut response = Response::builder().status(StatusCode::OK).body(()).unwrap();
        response.headers_mut().insert(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("SAMEORIGIN"),
        );
        Ok::<_, std::convert::Infallible>(response)
    }));

    let response = service.oneshot(Request::new(())).await.unwrap();
    assert_eq!(
        response.headers().get(header::X_FRAME_OPTIONS).unwrap(),
        "SAMEORIGIN"
    );
    assert!(
        response
            .headers()
            .contains_key(header::STRICT_TRANSPORT_SECURITY)
    );
    assert!(
        response
            .headers()
            .contains_key(header::X_CONTENT_TYPE_OPTIONS)
    );
}
