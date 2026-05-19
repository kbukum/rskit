//! Tower middleware for header-only bearer-token authentication.

use std::{future::Future, marker::PhantomData, pin::Pin, sync::Arc};

use http::{Request, Response, StatusCode, header};
use tower::{Layer, Service};

use crate::{AuthClaims, AuthOutcome, MissingCredentialPolicy, traits::TokenValidator};

/// Tower layer that validates `Authorization: Bearer <token>` headers.
#[derive(Clone)]
pub struct BearerAuthLayer<V, C> {
    validator: Arc<V>,
    missing_policy: MissingCredentialPolicy,
    _claims: PhantomData<fn() -> C>,
}

impl<V: 'static, C> BearerAuthLayer<V, C> {
    /// Create a new bearer-auth layer that rejects missing credentials.
    #[must_use]
    pub fn new(validator: V) -> Self {
        Self {
            validator: Arc::new(validator),
            missing_policy: MissingCredentialPolicy::RejectMissing,
            _claims: PhantomData,
        }
    }

    /// Explicitly accept requests with no credentials.
    #[must_use]
    pub const fn accept_missing(mut self) -> Self {
        self.missing_policy = MissingCredentialPolicy::AcceptMissing;
        self
    }
}

impl<S, V, C> Layer<S> for BearerAuthLayer<V, C>
where
    V: 'static,
{
    type Service = BearerAuthService<S, V, C>;

    fn layer(&self, inner: S) -> Self::Service {
        BearerAuthService {
            inner,
            validator: Arc::clone(&self.validator),
            missing_policy: self.missing_policy,
            _claims: PhantomData,
        }
    }
}

/// Tower service produced by [`BearerAuthLayer`].
#[derive(Clone)]
pub struct BearerAuthService<S, V, C> {
    inner: S,
    validator: Arc<V>,
    missing_policy: MissingCredentialPolicy,
    _claims: PhantomData<fn() -> C>,
}

impl<S, V, C, ReqBody, ResBody> Service<Request<ReqBody>> for BearerAuthService<S, V, C>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    V: TokenValidator<C> + 'static,
    C: Clone + Send + Sync + 'static,
    ReqBody: Send + 'static,
    ResBody: Default + Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

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
        let missing_policy = self.missing_policy;

        Box::pin(async move {
            match extract_bearer_token(&req) {
                CredentialExtraction::Missing => {
                    if missing_policy == MissingCredentialPolicy::AcceptMissing {
                        let mut req = req;
                        req.extensions_mut().insert(AuthOutcome::<C>::Missing);
                        inner.call(req).await
                    } else {
                        Ok(unauthorized_bearer_response())
                    }
                }
                CredentialExtraction::Invalid => Ok(unauthorized_bearer_response()),
                CredentialExtraction::Present(token) => match validator.validate(token).await {
                    Ok(claims) => {
                        let mut req = req;
                        req.extensions_mut().insert(AuthClaims(claims.clone()));
                        req.extensions_mut()
                            .insert(AuthOutcome::Authenticated(claims));
                        inner.call(req).await
                    }
                    Err(_) => Ok(unauthorized_bearer_response()),
                },
            }
        })
    }
}

enum CredentialExtraction<'a> {
    Missing,
    Invalid,
    Present(&'a str),
}

fn extract_bearer_token<B>(req: &Request<B>) -> CredentialExtraction<'_> {
    let mut values = req.headers().get_all(header::AUTHORIZATION).iter();
    let Some(value) = values.next() else {
        return CredentialExtraction::Missing;
    };
    if values.next().is_some() {
        return CredentialExtraction::Invalid;
    }
    let Ok(value) = value.to_str() else {
        return CredentialExtraction::Invalid;
    };
    let Some(token) = value.strip_prefix("Bearer ") else {
        return CredentialExtraction::Invalid;
    };
    if token.is_empty() || token.chars().any(char::is_whitespace) {
        return CredentialExtraction::Invalid;
    }
    CredentialExtraction::Present(token)
}

fn unauthorized_bearer_response<ResBody: Default>() -> Response<ResBody> {
    let mut response = Response::new(ResBody::default());
    *response.status_mut() = StatusCode::UNAUTHORIZED;
    response.headers_mut().insert(
        header::WWW_AUTHENTICATE,
        http::HeaderValue::from_static(r#"Bearer realm="rskit""#),
    );
    response
}

#[cfg(test)]
mod tests {
    use std::{convert::Infallible, future::Ready};

    use async_trait::async_trait;
    use http::{Request, Response, StatusCode};
    use rskit_errors::{AppError, AppResult};
    use serde::{Deserialize, Serialize};
    use tower::{Layer, Service};

    use super::{BearerAuthLayer, CredentialExtraction, extract_bearer_token};
    use crate::{AuthClaims, AuthOutcome, TokenValidator};

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct Claims {
        sub: String,
    }

    struct Validator;

    #[async_trait]
    impl TokenValidator<Claims> for Validator {
        async fn validate(&self, token: &str) -> AppResult<Claims> {
            if token == "good.token" {
                Ok(Claims {
                    sub: "user-1".into(),
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
            let has_claims = request.extensions().get::<AuthClaims<Claims>>().is_some();
            let has_outcome = request.extensions().get::<AuthOutcome<Claims>>().is_some();
            let status = if has_claims && has_outcome {
                StatusCode::OK
            } else if matches!(
                request.extensions().get::<AuthOutcome<Claims>>(),
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
    fn bearer_extraction_requires_single_authorization_header() {
        let request = http::Request::builder()
            .header(http::header::AUTHORIZATION, "Bearer abc.def.ghi")
            .body(())
            .unwrap();

        assert!(matches!(
            extract_bearer_token(&request),
            CredentialExtraction::Present("abc.def.ghi")
        ));

        let request = http::Request::builder()
            .header(http::header::AUTHORIZATION, "Bearer one")
            .header(http::header::AUTHORIZATION, "Bearer two")
            .body(())
            .unwrap();

        assert!(matches!(
            extract_bearer_token(&request),
            CredentialExtraction::Invalid
        ));
    }

    #[test]
    fn bearer_extraction_rejects_missing_or_malformed_values() {
        let missing = http::Request::builder().body(()).unwrap();
        assert!(matches!(
            extract_bearer_token(&missing),
            CredentialExtraction::Missing
        ));

        for value in ["bearer token", "Bearer ", "Bearer token with-space"] {
            let request = http::Request::builder()
                .header(http::header::AUTHORIZATION, value)
                .body(())
                .unwrap();
            assert!(matches!(
                extract_bearer_token(&request),
                CredentialExtraction::Invalid
            ));
        }
    }

    #[tokio::test]
    async fn bearer_layer_rejects_missing_by_default_and_accepts_valid_tokens() {
        let mut service =
            BearerAuthLayer::<_, Claims>::new(Validator).layer(ExtensionCheckingService);

        let missing = service
            .call(Request::builder().body(()).unwrap())
            .await
            .unwrap();
        assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);

        let valid = service
            .call(
                Request::builder()
                    .header(http::header::AUTHORIZATION, "Bearer good.token")
                    .body(())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(valid.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn bearer_layer_accept_missing_is_explicit_and_invalid_still_fails() {
        let mut service = BearerAuthLayer::<_, Claims>::new(Validator)
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
                    .header(http::header::AUTHORIZATION, "Bearer bad.token")
                    .body(())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(invalid.status(), StatusCode::UNAUTHORIZED);
    }
}
