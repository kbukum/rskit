use http::StatusCode;
use rskit_errors::{AppError, ErrorCode};

/// Framework-neutral HTTP request type.
pub type HttpRequest<B = Vec<u8>> = http::Request<B>;

/// Framework-neutral HTTP response type.
pub type HttpResponse<B = Vec<u8>> = http::Response<B>;

/// Framework-neutral HTTP header collection.
pub type HttpHeaders = http::HeaderMap;

/// Framework-neutral HTTP status code.
pub type HttpStatusCode = http::StatusCode;

/// Convert an HTTP status code into the canonical rskit error code.
#[must_use]
pub fn status_to_error_code(status: StatusCode) -> ErrorCode {
    match status.as_u16() {
        400 => ErrorCode::InvalidInput,
        401 => ErrorCode::Unauthorized,
        403 => ErrorCode::Forbidden,
        404 => ErrorCode::NotFound,
        409 => ErrorCode::Conflict,
        429 => ErrorCode::RateLimited,
        408 => ErrorCode::Cancelled,
        500 | 502 | 503 | 504 => ErrorCode::Internal,
        _ if status.is_client_error() || status.is_server_error() => ErrorCode::ExternalService,
        _ => ErrorCode::Internal,
    }
}

/// Return `true` when a status is in the successful 2xx range.
#[must_use]
pub fn is_success_status(status: StatusCode) -> bool {
    status.is_success()
}

/// Return the HTTP status code carried by an [`AppError`].
#[must_use]
pub fn app_error_status(error: &AppError) -> StatusCode {
    error.http_status()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_http_status_to_error_code() {
        assert_eq!(
            status_to_error_code(StatusCode::BAD_REQUEST),
            ErrorCode::InvalidInput
        );
        assert_eq!(
            status_to_error_code(StatusCode::NOT_FOUND),
            ErrorCode::NotFound
        );
        assert_eq!(
            status_to_error_code(StatusCode::TOO_MANY_REQUESTS),
            ErrorCode::RateLimited
        );
        assert_eq!(
            status_to_error_code(StatusCode::BAD_GATEWAY),
            ErrorCode::Internal
        );
    }

    #[test]
    fn cancelled_status_round_trips_through_error_code() {
        let status = ErrorCode::Cancelled.http_status();
        assert_eq!(status_to_error_code(status), ErrorCode::Cancelled);
    }

    #[test]
    fn exposes_protocol_types_without_framework_coupling() {
        let request: HttpRequest = http::Request::builder()
            .uri("/healthz")
            .body(Vec::new())
            .unwrap();
        let response: HttpResponse = http::Response::builder()
            .status(StatusCode::OK)
            .body(Vec::new())
            .unwrap();

        assert_eq!(request.uri(), "/healthz");
        assert!(is_success_status(response.status()));
    }
}
