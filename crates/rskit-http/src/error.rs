use axum::{
    body::Body,
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
};
use rskit_errors::{AppError, ErrorResponse};
use tower::{Layer, Service};

// ── axum IntoResponse for AppError ────────────────────────────────────────────

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status =
            StatusCode::from_u16(self.http_status.as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let body = serde_json::to_string(&ErrorResponse::from(&self)).unwrap_or_default();
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
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Response, Self::Error>> + Send>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let fut = self.inner.call(req);
        Box::pin(async move { fut.await })
    }
}
