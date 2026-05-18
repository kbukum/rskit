//! HTTP response security headers.

use std::sync::Arc;
use std::task::{Context, Poll};

use http::{HeaderName, HeaderValue, Request, Response};
use rskit_errors::AppResult;
pub use rskit_security::{SecurityHeadersConfig, TransportSecurity};
use tower::{Layer, Service};

/// Tower layer that applies secure response headers.
#[derive(Debug, Clone)]
pub struct SecurityHeadersLayer {
    headers: Arc<Vec<(HeaderName, HeaderValue)>>,
}

impl SecurityHeadersLayer {
    /// Create a new layer from validated configuration.
    ///
    /// # Errors
    /// Returns an error when the configuration is invalid or a header value cannot be constructed.
    pub fn new(config: &SecurityHeadersConfig) -> AppResult<Self> {
        let headers = Arc::new(config.header_pairs()?);
        Ok(Self { headers })
    }
}

impl<S> Layer<S> for SecurityHeadersLayer {
    type Service = SecurityHeadersService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        SecurityHeadersService {
            inner,
            headers: Arc::clone(&self.headers),
        }
    }
}

/// Service produced by [`SecurityHeadersLayer`].
#[derive(Debug, Clone)]
pub struct SecurityHeadersService<S> {
    inner: S,
    headers: Arc<Vec<(HeaderName, HeaderValue)>>,
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
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);
        let headers = Arc::clone(&self.headers);
        Box::pin(async move {
            let mut response = inner.call(req).await?;
            let response_headers = response.headers_mut();
            for (name, value) in headers.iter() {
                response_headers
                    .entry(name)
                    .or_insert_with(|| value.clone());
            }
            Ok(response)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::{
        StatusCode,
        header::{
            CONTENT_SECURITY_POLICY, HeaderValue, REFERRER_POLICY, STRICT_TRANSPORT_SECURITY,
            X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
        },
    };
    use tower::{ServiceBuilder, ServiceExt, service_fn};

    #[tokio::test]
    async fn defaults_apply_secure_headers() {
        let service = ServiceBuilder::new()
            .layer(SecurityHeadersLayer::new(&SecurityHeadersConfig::default()).unwrap())
            .service(service_fn(|_req: Request<()>| async {
                Ok::<_, std::convert::Infallible>(
                    Response::builder().status(StatusCode::OK).body(()).unwrap(),
                )
            }));

        let response = service.oneshot(Request::new(())).await.unwrap();
        let headers = response.headers();

        assert_eq!(
            headers.get(STRICT_TRANSPORT_SECURITY),
            Some(&HeaderValue::from_static(
                "max-age=63072000; includeSubDomains; preload"
            ))
        );
        assert_eq!(
            headers.get(X_CONTENT_TYPE_OPTIONS),
            Some(&HeaderValue::from_static("nosniff"))
        );
        assert_eq!(
            headers.get(X_FRAME_OPTIONS),
            Some(&HeaderValue::from_static("DENY"))
        );
    }

    #[tokio::test]
    async fn insecure_local_mode_omits_hsts() {
        let service = ServiceBuilder::new()
            .layer(
                SecurityHeadersLayer::new(
                    &SecurityHeadersConfig::default()
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
            .layer(SecurityHeadersLayer::new(&SecurityHeadersConfig::default()).unwrap())
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
