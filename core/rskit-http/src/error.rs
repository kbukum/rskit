use axum::{
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
};
use rskit_errors::{AppError, ProblemDetail};
use tower::{Layer, Service};

// ── HttpError ─────────────────────────────────────────────────────────────────

/// Wrapper around [`AppError`] that implements axum's [`IntoResponse`].
///
/// The default [`IntoResponse`] impl uses the `https://rskit.dev/errors/` type
/// URI base.  Production services should use [`HttpError::with_type_base_uri`]
/// (or build a `ProblemDetail` explicitly) so the `type` field in the response
/// body advertises the application's own error-documentation domain.
pub struct HttpError(pub AppError);

impl From<AppError> for HttpError {
    fn from(err: AppError) -> Self {
        Self(err)
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.0.code.http_status().as_u16())
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let body = serde_json::to_string(&ProblemDetail::from(&self.0)).unwrap_or_default();
        (status, [("content-type", "application/problem+json")], body).into_response()
    }
}

impl HttpError {
    /// Build an axum [`Response`] using `base_uri` as the RFC 9457 `type` prefix.
    ///
    /// Use this in HTTP middleware / error handlers that know the application's
    /// configured error-documentation domain.
    ///
    /// ```rust,ignore
    /// return HttpError::into_response_with_type_base(err, "https://myapp.com/errors");
    /// ```
    pub fn into_response_with_type_base(err: AppError, base_uri: &str) -> Response {
        let status = StatusCode::from_u16(err.code.http_status().as_u16())
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let body = serde_json::to_string(&ProblemDetail::with_base_uri(base_uri, &err))
            .unwrap_or_default();
        (status, [("content-type", "application/problem+json")], body).into_response()
    }
}

// ── Error handler Tower layer ─────────────────────────────────────────────────

/// Tower layer that converts unhandled errors into JSON [`AppError`] responses.
#[derive(Debug, Clone, Copy)]
pub struct ErrorHandlerLayer;

impl<S> Layer<S> for ErrorHandlerLayer {
    type Service = ErrorHandlerService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        ErrorHandlerService { inner }
    }
}

/// Service produced by [`ErrorHandlerLayer`].
#[derive(Debug, Clone)]
pub struct ErrorHandlerService<S> {
    inner: S,
}

impl<S, ReqBody> Service<Request<ReqBody>> for ErrorHandlerService<S>
where
    S: Service<Request<ReqBody>, Response = Response, Error = std::convert::Infallible>
        + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
{
    type Response = Response;
    type Error = std::convert::Infallible;
    type Future =
        std::pin::Pin<Box<dyn std::future::Future<Output = Result<Response, Self::Error>> + Send>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let fut = self.inner.call(req);
        Box::pin(fut)
    }
}
