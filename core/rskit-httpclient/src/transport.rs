//! Low-level transport helpers: redirect policy, bounded body reads, error and header mapping.

use std::error::Error;

use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::config::HttpClientConfig;

pub(crate) fn redirect_policy(config: &HttpClientConfig) -> reqwest::redirect::Policy {
    if !config.follow_redirects {
        return reqwest::redirect::Policy::none();
    }

    let max_redirects = config.max_redirects;
    let destination_policy = config.destination_policy.clone();
    reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() > max_redirects {
            return attempt.error(AppError::invalid_input(
                "max_redirects",
                format!("too many HTTP redirects (max {max_redirects})"),
            ));
        }
        if let Err(error) = destination_policy.validate(attempt.url()) {
            return attempt.error(error);
        }
        attempt.follow()
    })
}

pub(crate) async fn read_response_body(
    response: &mut reqwest::Response,
    max_bytes: usize,
) -> AppResult<bytes::Bytes> {
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes as u64)
    {
        return Err(response_body_too_large(max_bytes));
    }

    let mut total = 0usize;
    let mut body = bytes::BytesMut::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| {
        AppError::new(
            ErrorCode::ExternalService,
            format!("failed to read response body: {error}"),
        )
        .with_cause(error)
    })? {
        total = total
            .checked_add(chunk.len())
            .ok_or_else(|| response_body_too_large(max_bytes))?;
        if total > max_bytes {
            return Err(response_body_too_large(max_bytes));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body.freeze())
}

fn response_body_too_large(max_bytes: usize) -> AppError {
    AppError::invalid_input(
        "max_response_body_bytes",
        format!("HTTP response body exceeds configured limit of {max_bytes} bytes"),
    )
}

pub(crate) fn map_transport_error(error: reqwest::Error) -> AppError {
    if let Some(policy_error) = error
        .source()
        .and_then(|source| source.downcast_ref::<AppError>())
    {
        return AppError::new(policy_error.code(), policy_error.message()).with_cause(error);
    }

    let code = if error.is_timeout() {
        ErrorCode::Timeout
    } else if error.is_connect() {
        ErrorCode::ConnectionFailed
    } else {
        ErrorCode::ExternalService
    };
    AppError::new(code, format!("http request failed: {error}")).with_cause(error)
}

pub(crate) fn parse_header_name(name: &str) -> AppResult<reqwest::header::HeaderName> {
    name.parse::<reqwest::header::HeaderName>()
        .map_err(|error| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("invalid HTTP header name '{name}': {error}"),
            )
            .with_cause(error)
        })
}

pub(crate) fn parse_header_value(
    name: &str,
    value: &str,
) -> AppResult<reqwest::header::HeaderValue> {
    value
        .parse::<reqwest::header::HeaderValue>()
        .map_err(|error| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("invalid HTTP header value for '{name}': {error}"),
            )
            .with_cause(error)
        })
}
