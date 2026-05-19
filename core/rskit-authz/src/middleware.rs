//! Tower middleware for request-scoped authorization checks.

use std::{future::Future, pin::Pin, sync::Arc};

use http::{Request as HttpRequest, Response, StatusCode};
use tower::{Layer, Service};

use crate::{Checker, Request};

/// Builds an authorization request from an HTTP request.
pub trait RequestAuthorizer<B>: Send + Sync {
    /// Return an authorization request, or `None` when required context is missing.
    fn authorization_request(&self, request: &HttpRequest<B>) -> Option<Request>;
}

impl<B, F> RequestAuthorizer<B> for F
where
    F: Fn(&HttpRequest<B>) -> Option<Request> + Send + Sync,
{
    fn authorization_request(&self, request: &HttpRequest<B>) -> Option<Request> {
        self(request)
    }
}

/// Tower layer that denies requests unless the checker explicitly allows them.
#[derive(Clone)]
pub struct AuthorizationLayer<C, A> {
    checker: Arc<C>,
    authorizer: Arc<A>,
}

impl<C, A> AuthorizationLayer<C, A> {
    /// Create an authorization layer from a checker and request mapper.
    #[must_use]
    pub fn new(checker: C, authorizer: A) -> Self {
        Self {
            checker: Arc::new(checker),
            authorizer: Arc::new(authorizer),
        }
    }
}

impl<S, C, A> Layer<S> for AuthorizationLayer<C, A> {
    type Service = AuthorizationService<S, C, A>;

    fn layer(&self, inner: S) -> Self::Service {
        AuthorizationService {
            inner,
            checker: Arc::clone(&self.checker),
            authorizer: Arc::clone(&self.authorizer),
        }
    }
}

/// Tower service produced by [`AuthorizationLayer`].
#[derive(Clone)]
pub struct AuthorizationService<S, C, A> {
    inner: S,
    checker: Arc<C>,
    authorizer: Arc<A>,
}

impl<S, C, A, ReqBody, ResBody> Service<HttpRequest<ReqBody>> for AuthorizationService<S, C, A>
where
    S: Service<HttpRequest<ReqBody>, Response = Response<ResBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    C: Checker + 'static,
    A: RequestAuthorizer<ReqBody> + 'static,
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

    fn call(&mut self, req: HttpRequest<ReqBody>) -> Self::Future {
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);
        let checker = Arc::clone(&self.checker);
        let authorizer = Arc::clone(&self.authorizer);

        Box::pin(async move {
            let Some(authz_request) = authorizer.authorization_request(&req) else {
                return Ok(forbidden_response());
            };
            if checker.check(&authz_request) {
                inner.call(req).await
            } else {
                Ok(forbidden_response())
            }
        })
    }
}

fn forbidden_response<ResBody: Default>() -> Response<ResBody> {
    let mut response = Response::new(ResBody::default());
    *response.status_mut() = StatusCode::FORBIDDEN;
    response
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, convert::Infallible, future::Ready};

    use http::{Request as HttpRequest, Response, StatusCode};
    use tower::{Layer, Service};

    use super::{AuthorizationLayer, RequestAuthorizer};
    use crate::{Checker, Decision, Request, Resource, Subject};

    #[derive(Clone)]
    struct StaticChecker(bool);

    impl Checker for StaticChecker {
        fn authorize(&self, _request: &Request) -> Decision {
            Decision {
                allowed: self.0,
                reason: "test".into(),
            }
        }
    }

    #[derive(Clone)]
    struct OkService;

    impl Service<HttpRequest<()>> for OkService {
        type Response = Response<()>;
        type Error = Infallible;
        type Future = Ready<Result<Self::Response, Self::Error>>;

        fn poll_ready(
            &mut self,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Ready(Ok(()))
        }

        fn call(&mut self, _request: HttpRequest<()>) -> Self::Future {
            std::future::ready(Ok(Response::builder()
                .status(StatusCode::OK)
                .body(())
                .unwrap()))
        }
    }

    fn authz_request() -> Request {
        Request {
            subject: Subject {
                id: "user-1".into(),
                roles: Vec::new(),
                attributes: HashMap::new(),
            },
            resource: Resource {
                resource_type: "article".into(),
                id: "article-1".into(),
                attributes: HashMap::new(),
            },
            action: "read".into(),
            context: HashMap::new(),
        }
    }

    #[derive(Clone)]
    struct StaticAuthorizer(bool);

    impl RequestAuthorizer<()> for StaticAuthorizer {
        fn authorization_request(&self, _request: &HttpRequest<()>) -> Option<Request> {
            self.0.then(authz_request)
        }
    }

    #[tokio::test]
    async fn authorization_layer_allows_only_explicit_allow_decisions() {
        let mut service =
            AuthorizationLayer::new(StaticChecker(true), StaticAuthorizer(true)).layer(OkService);
        let response = service.call(HttpRequest::new(())).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let mut service =
            AuthorizationLayer::new(StaticChecker(false), StaticAuthorizer(true)).layer(OkService);
        let response = service.call(HttpRequest::new(())).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn authorization_layer_denies_missing_authorization_context() {
        let mut service =
            AuthorizationLayer::new(StaticChecker(true), StaticAuthorizer(false)).layer(OkService);
        let response = service.call(HttpRequest::new(())).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}
