//! Tower middleware for API key validation.

use async_trait::async_trait;
use http::{Request, Response, StatusCode, header::HeaderName};
use rskit_errors::AppError;
use std::sync::Arc;
use tower::{Layer, Service};

use super::Key;
use crate::{AuthOutcome, MissingCredentialPolicy};

/// Validates an API key and returns its metadata.
#[async_trait]
pub trait KeyValidator: Send + Sync {
    /// Look up a key by its plaintext value, validate it, and return metadata.
    async fn validate_key(&self, plain_key: &str) -> Result<Key, AppError>;
}

/// Tower Layer for API key validation.
///
/// If the configured header is absent,
/// the request is rejected unless [`ApiKeyLayer::accept_missing`] is enabled. If present but invalid,
/// returns 401.
#[derive(Clone)]
pub struct ApiKeyLayer<V> {
    validator: Arc<V>,
    header_name: HeaderName,
    missing_policy: MissingCredentialPolicy,
}

impl<V: KeyValidator + 'static> ApiKeyLayer<V> {
    /// Create a new layer using the default `x-api-key` header.
    pub fn new(validator: V) -> Self {
        Self {
            validator: Arc::new(validator),
            header_name: HeaderName::from_static("x-api-key"),
            missing_policy: MissingCredentialPolicy::RejectMissing,
        }
    }

    /// Override the header name used for key extraction.
    #[must_use]
    pub fn with_header(mut self, name: HeaderName) -> Self {
        self.header_name = name;
        self
    }

    /// Explicitly accept requests with no API-key header.
    #[must_use]
    pub const fn accept_missing(mut self) -> Self {
        self.missing_policy = MissingCredentialPolicy::AcceptMissing;
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
            missing_policy: self.missing_policy,
        }
    }
}

#[derive(Clone)]
pub struct ApiKeyService<S, V> {
    inner: S,
    validator: Arc<V>,
    header_name: HeaderName,
    missing_policy: MissingCredentialPolicy,
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
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);
        let validator = Arc::clone(&self.validator);
        let header_name = self.header_name.clone();
        let missing_policy = self.missing_policy;

        Box::pin(async move {
            match extract_api_key(&req, &header_name) {
                CredentialExtraction::Missing => {
                    if missing_policy == MissingCredentialPolicy::AcceptMissing {
                        let mut req = req;
                        req.extensions_mut().remove::<Key>();
                        req.extensions_mut().insert(AuthOutcome::<Key>::Missing);
                        inner.call(req).await
                    } else {
                        Ok(unauthorized_response())
                    }
                }
                CredentialExtraction::Invalid => Ok(unauthorized_response()),
                CredentialExtraction::Present(plain_key) => {
                    if let Ok(key) = validator.validate_key(plain_key).await {
                        let mut req = req;
                        req.extensions_mut().insert(key.clone());
                        req.extensions_mut().insert(AuthOutcome::Authenticated(key));
                        inner.call(req).await
                    } else {
                        Ok(unauthorized_response())
                    }
                }
            }
        })
    }
}

enum CredentialExtraction<'a> {
    Missing,
    Invalid,
    Present(&'a str),
}

fn extract_api_key<'a, B>(
    req: &'a Request<B>,
    header_name: &HeaderName,
) -> CredentialExtraction<'a> {
    let mut values = req.headers().get_all(header_name).iter();
    let Some(value) = values.next() else {
        return CredentialExtraction::Missing;
    };
    if values.next().is_some() {
        return CredentialExtraction::Invalid;
    }
    let Ok(value) = value.to_str() else {
        return CredentialExtraction::Invalid;
    };
    if value.trim().is_empty() || value.chars().any(char::is_whitespace) {
        return CredentialExtraction::Invalid;
    }
    CredentialExtraction::Present(value)
}

fn unauthorized_response<ResBody: Default>() -> Response<ResBody> {
    let mut response = Response::new(ResBody::default());
    *response.status_mut() = StatusCode::UNAUTHORIZED;
    response
}

#[cfg(test)]
mod tests {
    use std::{convert::Infallible, future::Ready};

    use async_trait::async_trait;
    use chrono::Utc;
    use http::{Request, Response, StatusCode};
    use rskit_errors::AppError;
    use tower::{Layer, Service, ServiceExt};

    use super::{ApiKeyLayer, CredentialExtraction, KeyValidator, extract_api_key};
    use crate::{AuthOutcome, apikey::Key};

    struct Validator;

    #[async_trait]
    impl KeyValidator for Validator {
        async fn validate_key(&self, plain_key: &str) -> Result<Key, AppError> {
            if plain_key == "pk.secret" {
                Ok(Key {
                    id: "key-1".into(),
                    owner_id: "user-1".into(),
                    name: "primary".into(),
                    key_prefix: "pk".into(),
                    key_digest: "digest".into(),
                    scopes: vec!["read".into()],
                    is_active: true,
                    expires_at: None,
                    grace_ends_at: None,
                    rotated_by_id: None,
                    last_used_at: None,
                    created_at: Utc::now(),
                })
            } else {
                Err(AppError::invalid_token())
            }
        }
    }

    #[derive(Clone)]
    struct ExtensionCheckingService;

    impl Service<Request<()>> for ExtensionCheckingService {
        type Response = Response<()>;
        type Error = Infallible;
        type Future = Ready<Result<Self::Response, Self::Error>>;

        fn poll_ready(
            &mut self,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Ready(Ok(()))
        }

        fn call(&mut self, request: Request<()>) -> Self::Future {
            let status = if request.extensions().get::<Key>().is_some()
                && request.extensions().get::<AuthOutcome<Key>>().is_some()
            {
                StatusCode::OK
            } else if matches!(
                request.extensions().get::<AuthOutcome<Key>>(),
                Some(AuthOutcome::Missing)
            ) {
                StatusCode::NO_CONTENT
            } else {
                StatusCode::IM_A_TEAPOT
            };
            std::future::ready(Ok(Response::builder().status(status).body(()).unwrap()))
        }
    }

    #[test]
    fn api_key_extraction_requires_single_non_empty_header() {
        let header = http::header::HeaderName::from_static("x-api-key");
        let request = http::Request::builder()
            .header(&header, "pk.secret")
            .body(())
            .unwrap();
        assert!(matches!(
            extract_api_key(&request, &header),
            CredentialExtraction::Present("pk.secret")
        ));

        let request = http::Request::builder()
            .header(&header, "one")
            .header(&header, "two")
            .body(())
            .unwrap();
        assert!(matches!(
            extract_api_key(&request, &header),
            CredentialExtraction::Invalid
        ));
    }

    #[test]
    fn api_key_extraction_rejects_blank_or_whitespace_values() {
        let header = http::header::HeaderName::from_static("x-api-key");
        let missing = http::Request::builder().body(()).unwrap();
        assert!(matches!(
            extract_api_key(&missing, &header),
            CredentialExtraction::Missing
        ));

        for value in ["", "   ", "pk.secret with-space"] {
            let request = http::Request::builder()
                .header(&header, value)
                .body(())
                .unwrap();
            assert!(matches!(
                extract_api_key(&request, &header),
                CredentialExtraction::Invalid
            ));
        }
    }

    #[tokio::test]
    async fn api_key_layer_rejects_missing_by_default_and_accepts_valid_keys() {
        let mut service = ApiKeyLayer::new(Validator).layer(ExtensionCheckingService);

        let missing = service
            .ready()
            .await
            .unwrap()
            .call(Request::builder().body(()).unwrap())
            .await
            .unwrap();
        assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);

        let valid = service
            .ready()
            .await
            .unwrap()
            .call(
                Request::builder()
                    .header("x-api-key", "pk.secret")
                    .body(())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(valid.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn api_key_layer_uses_configured_header_name() {
        let mut service = ApiKeyLayer::new(Validator)
            .with_header(http::header::HeaderName::from_static("x-service-key"))
            .layer(ExtensionCheckingService);

        let default_header = service
            .ready()
            .await
            .unwrap()
            .call(
                Request::builder()
                    .header("x-api-key", "pk.secret")
                    .body(())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(default_header.status(), StatusCode::UNAUTHORIZED);

        let custom_header = service
            .ready()
            .await
            .unwrap()
            .call(
                Request::builder()
                    .header("x-service-key", "pk.secret")
                    .body(())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(custom_header.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn api_key_layer_accept_missing_does_not_accept_invalid_keys() {
        let mut service = ApiKeyLayer::new(Validator)
            .accept_missing()
            .layer(ExtensionCheckingService);

        let missing = service
            .call(Request::builder().body(()).unwrap())
            .await
            .unwrap();
        assert_eq!(missing.status(), StatusCode::NO_CONTENT);

        let invalid = service
            .call(
                Request::builder()
                    .header("x-api-key", "pk.bad")
                    .body(())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(invalid.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn api_key_layer_accept_missing_clears_stale_key() {
        let mut service = ApiKeyLayer::new(Validator)
            .accept_missing()
            .layer(ExtensionCheckingService);
        let mut request = Request::builder().body(()).unwrap();
        request.extensions_mut().insert(Key {
            id: "stale-key".into(),
            owner_id: "user-1".into(),
            name: "stale".into(),
            key_prefix: "pk".into(),
            key_digest: "digest".into(),
            scopes: vec!["read".into()],
            is_active: true,
            expires_at: None,
            grace_ends_at: None,
            rotated_by_id: None,
            last_used_at: None,
            created_at: Utc::now(),
        });

        let response = service.call(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }
}
