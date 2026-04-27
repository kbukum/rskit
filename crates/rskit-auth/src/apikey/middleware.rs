//! Tower middleware for API key validation.

use async_trait::async_trait;
use http::{Request, Response, StatusCode, header::HeaderName};
use rskit_errors::AppError;
use std::sync::Arc;
use tower::{Layer, Service};

use super::Key;

/// Validates an API key and returns its metadata.
#[async_trait]
pub trait KeyValidator: Send + Sync {
    /// Look up a key by its plaintext value, validate it, and return metadata.
    async fn validate_key(&self, plain_key: &str) -> Result<Key, AppError>;
}

/// Tower Layer for API key validation.
///
/// If the configured header is absent, the request passes through.
/// If present but invalid, returns 401.
#[derive(Clone)]
pub struct ApiKeyLayer<V> {
    validator: Arc<V>,
    header_name: HeaderName,
}

impl<V: KeyValidator + 'static> ApiKeyLayer<V> {
    /// Create a new layer using the default `x-api-key` header.
    pub fn new(validator: V) -> Self {
        Self {
            validator: Arc::new(validator),
            header_name: HeaderName::from_static("x-api-key"),
        }
    }

    /// Override the header name used for key extraction.
    #[must_use]
    pub fn with_header(mut self, name: HeaderName) -> Self {
        self.header_name = name;
        self
    }
}

impl<S, V> Layer<S> for ApiKeyLayer<V>
where
    V: KeyValidator + 'static,
{
    type Service = ApiKeyService<S, V>;

    fn layer(&self, inner: S) -> Self::Service {
        ApiKeyService {
            inner,
            validator: Arc::clone(&self.validator),
            header_name: self.header_name.clone(),
        }
    }
}

#[derive(Clone)]
pub struct ApiKeyService<S, V> {
    inner: S,
    validator: Arc<V>,
    header_name: HeaderName,
}

impl<S, V, ReqBody, ResBody> Service<Request<ReqBody>> for ApiKeyService<S, V>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    V: KeyValidator + 'static,
    ReqBody: Send + 'static,
    ResBody: Default + Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let mut inner = self.inner.clone();
        let validator = Arc::clone(&self.validator);
        let header_name = self.header_name.clone();

        Box::pin(async move {
            let raw_key = req.headers().get(&header_name);

            if raw_key.is_none() {
                return inner.call(req).await;
            }

            // SAFETY: checked is_none() above, so raw_key is guaranteed Some here.
            let Ok(plain_key) = raw_key.expect("checked above").to_str() else {
                let mut res = Response::new(ResBody::default());
                *res.status_mut() = StatusCode::UNAUTHORIZED;
                // Add WWW-Authenticate header per RFC 7235
                res.headers_mut().insert(
                    http::header::WWW_AUTHENTICATE,
                    http::HeaderValue::from_static(r#"Bearer realm="rskit""#),
                );
                return Ok(res);
            };

            if let Ok(_key) = validator.validate_key(plain_key).await {
                // Store key in request extensions if needed
                // For now, just pass through
                inner.call(req).await
            } else {
                let mut res = Response::new(ResBody::default());
                *res.status_mut() = StatusCode::UNAUTHORIZED;
                // Add WWW-Authenticate header per RFC 7235
                res.headers_mut().insert(
                    http::header::WWW_AUTHENTICATE,
                    http::HeaderValue::from_static(r#"Bearer realm="rskit""#),
                );
                Ok(res)
            }
        })
    }
}
