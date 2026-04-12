//! Cross-layer integration tests for rskit.
//!
//! Tests verify that modules work together correctly across architectural layers.
//! Each test exercises at least 2 crates from different layers using real APIs.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use rskit_auth::{JwtConfig, JwtService, TokenGenerator, TokenValidator};
use rskit_authz::{Checker, Effect, Policy, RbacChecker};
use rskit_bootstrap::{AppBuilder, Component, Health, HealthStatus, Registry};
use rskit_di::Container;
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_pipeline::{RskitStreamExt, from_slice};
use rskit_provider::{Provider, RequestResponse, request_response_fn};
use rskit_resilience::{CbConfig, CbState, CircuitBreaker};
use rskit_validation::Validator;
use serde::{Deserialize, Serialize};

// ─── Helpers ──────────────────────────────────────────────────────────────────

struct TrackingComponent {
    name: &'static str,
    started: AtomicBool,
    stopped: AtomicBool,
}

impl TrackingComponent {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            started: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
        }
    }
}

#[async_trait]
impl Component for TrackingComponent {
    fn name(&self) -> &str {
        self.name
    }
    async fn start(&self) -> AppResult<()> {
        self.started.store(true, Ordering::SeqCst);
        Ok(())
    }
    async fn stop(&self) -> AppResult<()> {
        self.stopped.store(true, Ordering::SeqCst);
        Ok(())
    }
    fn health(&self) -> Health {
        if self.started.load(Ordering::SeqCst) && !self.stopped.load(Ordering::SeqCst) {
            Health::healthy(self.name)
        } else {
            Health::unhealthy(self.name, "not running")
        }
    }
}

// Tracks the order of start/stop calls across components.
struct OrderTracker {
    events: parking_lot::Mutex<Vec<String>>,
}

impl OrderTracker {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            events: parking_lot::Mutex::new(Vec::new()),
        })
    }

    fn push(&self, event: &str) {
        self.events.lock().push(event.to_string());
    }

    fn events(&self) -> Vec<String> {
        self.events.lock().clone()
    }
}

struct OrderedComponent {
    name: String,
    tracker: Arc<OrderTracker>,
}

#[async_trait]
impl Component for OrderedComponent {
    fn name(&self) -> &str {
        &self.name
    }
    async fn start(&self) -> AppResult<()> {
        self.tracker.push(&format!("start:{}", self.name));
        Ok(())
    }
    async fn stop(&self) -> AppResult<()> {
        self.tracker.push(&format!("stop:{}", self.name));
        Ok(())
    }
    fn health(&self) -> Health {
        Health::healthy(&self.name)
    }
}

// ─── 1. Errors → Resilience ──────────────────────────────────────────────────

#[tokio::test]
async fn errors_resilience_circuit_breaker_preserves_error_code() {
    let cb = CircuitBreaker::new(
        CbConfig::new("test-cb")
            .with_max_failures(3)
            .with_timeout(Duration::from_millis(100)),
    );

    // Trip the breaker with AppErrors
    for _ in 0..3 {
        let result: AppResult<()> = cb
            .execute(|| async { Err(AppError::service_unavailable("database")) })
            .await;
        let err = result.unwrap_err();
        assert_eq!(err.code, ErrorCode::ServiceUnavailable);
        assert!(err.retryable, "SERVICE_UNAVAILABLE should be retryable");
    }

    assert_eq!(cb.state(), CbState::Open);

    // Calls fail fast when open
    let result: AppResult<()> = cb.execute(|| async { Ok(()) }).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn errors_resilience_circuit_breaker_recovery() {
    let cb = CircuitBreaker::new(
        CbConfig::new("recover-cb")
            .with_max_failures(2)
            .with_timeout(Duration::from_millis(50))
            .with_half_open_max_calls(1),
    );

    // Trip the breaker
    for _ in 0..2 {
        let _: AppResult<()> = cb
            .execute(|| async { Err(AppError::connection_failed("redis")) })
            .await;
    }
    assert_eq!(cb.state(), CbState::Open);

    tokio::time::sleep(Duration::from_millis(60)).await;

    // Successful probe should close the breaker
    let result: AppResult<String> = cb.execute(|| async { Ok("recovered".to_string()) }).await;
    assert_eq!(result.unwrap(), "recovered");
    assert_eq!(cb.state(), CbState::Closed);
}

#[tokio::test]
async fn errors_resilience_various_error_codes_through_breaker() {
    let cb = CircuitBreaker::new(
        CbConfig::new("codes-cb")
            .with_max_failures(10)
            .with_timeout(Duration::from_secs(1)),
    );

    // Different error codes pass through the circuit breaker
    let codes = [
        ErrorCode::Timeout,
        ErrorCode::NotFound,
        ErrorCode::Unauthorized,
        ErrorCode::InvalidInput,
    ];
    for code in &codes {
        let c = *code;
        let result: AppResult<()> = cb
            .execute(move || async move { Err(AppError::new(c, "test error")) })
            .await;
        let err = result.unwrap_err();
        assert_eq!(err.code, c, "error code should be preserved through CB");
    }
}

// ─── 2. Config → Bootstrap ──────────────────────────────────────────────────

#[derive(Debug, Deserialize, validator::Validate, Default)]
struct TestConfig {
    #[serde(default)]
    service: rskit_config::ServiceConfig,
}

impl rskit_config::AppConfig for TestConfig {
    fn apply_defaults(&mut self) {}
    fn service_config(&self) -> &rskit_config::ServiceConfig {
        &self.service
    }
}

#[tokio::test]
async fn config_bootstrap_components_start_in_order() {
    let tracker = OrderTracker::new();

    let db = Arc::new(OrderedComponent {
        name: "db".to_string(),
        tracker: tracker.clone(),
    });
    let cache = Arc::new(OrderedComponent {
        name: "cache".to_string(),
        tracker: tracker.clone(),
    });
    let api = Arc::new(OrderedComponent {
        name: "api".to_string(),
        tracker: tracker.clone(),
    });

    let config = TestConfig::default();
    let app = AppBuilder::new(config)
        .with_component(db)
        .with_component(cache)
        .with_component(api)
        .build()
        .expect("build should succeed");

    let result = app.run_task(|_cfg, _token| async { Ok(()) }).await;
    assert!(result.is_ok());

    let events = tracker.events();
    // Start order is registration order
    assert_eq!(&events[0], "start:db");
    assert_eq!(&events[1], "start:cache");
    assert_eq!(&events[2], "start:api");
    // Stop order is reverse
    assert_eq!(&events[3], "stop:api");
    assert_eq!(&events[4], "stop:cache");
    assert_eq!(&events[5], "stop:db");
}

#[tokio::test]
async fn config_bootstrap_health_check() {
    let comp = Arc::new(TrackingComponent::new("test-db"));

    let mut registry = Registry::new();
    registry.register(comp.clone());

    registry.start_all().await.unwrap();

    let health = comp.health();
    assert_eq!(health.status, HealthStatus::Healthy);

    registry.stop_all().await;

    let health = comp.health();
    assert_eq!(health.status, HealthStatus::Unhealthy);
}

// ─── 3. Provider → Pipeline ─────────────────────────────────────────────────

#[tokio::test]
async fn provider_pipeline_stream_through_operators() {
    use futures::StreamExt;

    let stream = from_slice(vec![1i32, 2, 3, 4, 5]);
    let results: Vec<AppResult<i32>> = stream
        .rmap(|x| async move { Ok(x * 2) })
        .rfilter(|r| r.as_ref().is_ok_and(|x| *x > 4))
        .collect()
        .await;

    let values: Vec<i32> = results.into_iter().map(|r| r.unwrap()).collect();
    assert_eq!(values, vec![6, 8, 10]);
}

#[tokio::test]
async fn provider_pipeline_map_filter_collect() {
    use futures::StreamExt;

    let stream = from_slice(vec!["alice", "bob", "charlie"]);
    let results: Vec<AppResult<String>> = stream
        .rmap(|name| async move { Ok(format!("user:{}", name)) })
        .rfilter(|r| r.as_ref().is_ok_and(|s| s != "user:bob"))
        .collect()
        .await;

    let values: Vec<String> = results.into_iter().map(|r| r.unwrap()).collect();
    assert_eq!(values, vec!["user:alice", "user:charlie"]);
}

#[tokio::test]
async fn provider_pipeline_request_response_fn() {
    let provider = request_response_fn("doubler", |x: i32| async move { Ok(x * 2) });

    assert_eq!(provider.name(), "doubler");
    let result = provider.execute(21).await.unwrap();
    assert_eq!(result, 42);
}

#[tokio::test]
async fn provider_pipeline_provider_feeds_stream() {
    use futures::StreamExt;

    let provider = request_response_fn("tripler", |x: i32| async move { Ok(x * 3) });

    // Simulate provider feeding data into pipeline
    let inputs = vec![1, 2, 3, 4, 5];
    let mut results = Vec::new();
    for input in inputs {
        results.push(provider.execute(input).await.unwrap());
    }

    let stream = from_slice(results);
    let filtered: Vec<i32> = stream.rfilter(|x| *x > 6).collect().await;
    assert_eq!(filtered, vec![9, 12, 15]);
}

// ─── 4. Validation → Errors ─────────────────────────────────────────────────

#[test]
fn validation_errors_produces_correct_app_error() {
    let result = Validator::new()
        .required("name", "")
        .email("email", "not-an-email")
        .validate();

    let err = result.unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
    assert_eq!(err.http_status, http::StatusCode::UNPROCESSABLE_ENTITY);
}

#[test]
fn validation_errors_multiple_fields() {
    let result = Validator::new()
        .required("username", "")
        .required("password", "")
        .email("contact", "bad")
        .validate();

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
}

#[test]
fn validation_errors_passing_validation() {
    let result = Validator::new()
        .required("name", "Alice")
        .email("email", "alice@example.com")
        .validate();

    assert!(result.is_ok());
}

#[test]
fn validation_errors_chained_checks() {
    let result = Validator::new()
        .required("id", "abc-123")
        .max_length("name", "Al", 100)
        .min_length("password", "secure-pass", 8)
        .validate();

    assert!(result.is_ok());
}

// ─── 5. DI → Component ─────────────────────────────────────────────────────

#[tokio::test]
async fn di_component_container_manages_lifecycle() {
    let container = Container::new();

    let db = Arc::new(TrackingComponent::new("postgres"));
    let cache = Arc::new(TrackingComponent::new("redis"));

    container.register::<TrackingComponent>(db.clone());

    let resolved: Arc<TrackingComponent> = container.resolve().unwrap();
    assert_eq!(resolved.name(), "postgres");

    // Register in registry and start
    let mut registry = Registry::new();
    registry.register(db.clone() as Arc<dyn Component>);
    registry.register(cache.clone() as Arc<dyn Component>);

    registry.start_all().await.unwrap();
    assert!(db.started.load(Ordering::SeqCst));
    assert!(cache.started.load(Ordering::SeqCst));

    registry.stop_all().await;
    assert!(db.stopped.load(Ordering::SeqCst));
    assert!(cache.stopped.load(Ordering::SeqCst));
}

#[test]
fn di_component_resolve_missing_returns_error() {
    let container = Container::new();
    let result = container.resolve::<String>();
    assert!(result.is_err());
}

#[test]
fn di_component_singleton_returns_same_instance() {
    let container = Container::new();
    let call_count = Arc::new(AtomicU32::new(0));
    let cc = call_count.clone();

    container.register_singleton::<String, _>(move || {
        cc.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new("singleton-value".to_string()))
    });

    let v1: Arc<String> = container.resolve().unwrap();
    let v2: Arc<String> = container.resolve().unwrap();

    assert_eq!(*v1, "singleton-value");
    assert_eq!(*v2, "singleton-value");
    assert_eq!(
        call_count.load(Ordering::SeqCst),
        1,
        "factory should be called only once"
    );
}

// ─── 6. Auth → Authz ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct TestClaims {
    sub: String,
    role: String,
    exp: u64,
}

fn future_exp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600
}

#[tokio::test]
async fn auth_authz_jwt_claims_feed_rbac() {
    let jwt_svc = JwtService::<TestClaims>::new(JwtConfig {
        secret: "integration-test-secret".into(),
        ..Default::default()
    });

    // Generate token with role
    let claims = TestClaims {
        sub: "user-1".into(),
        role: "admin".into(),
        exp: future_exp(),
    };
    let token = jwt_svc.generate(&claims).await.unwrap();
    let decoded = jwt_svc.validate(&token).await.unwrap();

    // Feed claims into RBAC checker
    let checker = RbacChecker::new(vec![
        Policy {
            subject: "admin".into(),
            action: "*".into(),
            resource: "*".into(),
            effect: Effect::Allow,
        },
        Policy {
            subject: "viewer".into(),
            action: "read".into(),
            resource: "*".into(),
            effect: Effect::Allow,
        },
    ]);

    // Admin should have wildcard access
    assert!(
        checker
            .check(&decoded.role, "delete", "users")
            .await
            .is_ok()
    );
    assert!(
        checker
            .check(&decoded.role, "write", "articles")
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn auth_authz_restricted_role() {
    let jwt_svc = JwtService::<TestClaims>::new(JwtConfig {
        secret: "restricted-secret".into(),
        ..Default::default()
    });

    let claims = TestClaims {
        sub: "user-2".into(),
        role: "viewer".into(),
        exp: future_exp(),
    };
    let token = jwt_svc.generate(&claims).await.unwrap();
    let decoded = jwt_svc.validate(&token).await.unwrap();

    let checker = RbacChecker::new(vec![Policy {
        subject: "viewer".into(),
        action: "read".into(),
        resource: "*".into(),
        effect: Effect::Allow,
    }]);

    assert!(
        checker
            .check(&decoded.role, "read", "articles")
            .await
            .is_ok()
    );
    assert!(
        checker
            .check(&decoded.role, "write", "articles")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn auth_authz_deny_overrides_allow() {
    let jwt_svc = JwtService::<TestClaims>::new(JwtConfig {
        secret: "deny-test-secret".into(),
        ..Default::default()
    });

    let claims = TestClaims {
        sub: "user-3".into(),
        role: "editor".into(),
        exp: future_exp(),
    };
    let token = jwt_svc.generate(&claims).await.unwrap();
    let decoded = jwt_svc.validate(&token).await.unwrap();

    let checker = RbacChecker::new(vec![
        Policy {
            subject: "editor".into(),
            action: "*".into(),
            resource: "articles".into(),
            effect: Effect::Allow,
        },
        Policy {
            subject: "editor".into(),
            action: "delete".into(),
            resource: "articles".into(),
            effect: Effect::Deny,
        },
    ]);

    assert!(
        checker
            .check(&decoded.role, "read", "articles")
            .await
            .is_ok()
    );
    assert!(
        checker
            .check(&decoded.role, "write", "articles")
            .await
            .is_ok()
    );
    // Deny should override the wildcard allow
    assert!(
        checker
            .check(&decoded.role, "delete", "articles")
            .await
            .is_err()
    );
}

// ─── 7. Errors → Validation → Pipeline ─────────────────────────────────────

#[tokio::test]
async fn errors_validation_pipeline_integration() {
    use futures::StreamExt;

    let inputs = vec![
        ("Alice", "alice@example.com"),
        ("", "bob@example.com"), // invalid: empty name
        ("Charlie", "charlie@test.com"),
    ];

    let stream = from_slice(inputs);
    let validated: Vec<AppResult<String>> = stream
        .rmap(|(name, email)| async move {
            Validator::new()
                .required("name", name)
                .email("email", email)
                .validate()?;
            Ok(format!("{} <{}>", name, email))
        })
        .collect()
        .await;

    assert!(validated[0].is_ok());
    assert!(validated[1].is_err());
    assert!(validated[2].is_ok());

    if let Err(ref err) = validated[1] {
        assert_eq!(err.code, ErrorCode::InvalidInput);
    }
}

// ─── 8. DI → Resilience ────────────────────────────────────────────────────

#[tokio::test]
async fn di_resilience_circuit_breaker_in_container() {
    let container = Container::new();
    let cb = Arc::new(CircuitBreaker::new(
        CbConfig::new("di-cb")
            .with_max_failures(3)
            .with_timeout(Duration::from_millis(100)),
    ));

    container.register::<CircuitBreaker>(cb.clone());
    let resolved: Arc<CircuitBreaker> = container.resolve().unwrap();

    let result: AppResult<String> = resolved.execute(|| async { Ok("hello".to_string()) }).await;
    assert_eq!(result.unwrap(), "hello");
    assert_eq!(resolved.state(), CbState::Closed);
}

// ─── 9. Full stack: Config → DI → Component → Provider ─────────────────────

#[tokio::test]
async fn full_stack_config_di_component_provider() {
    let container = Container::new();

    // Register a provider in DI
    let provider = request_response_fn("multiplier", |x: i32| async move { Ok(x * 3) });
    container.register(Arc::new(provider));

    // Create components
    let comp = Arc::new(TrackingComponent::new("worker"));
    let mut registry = Registry::new();
    registry.register(comp.clone());
    registry.start_all().await.unwrap();

    assert!(comp.started.load(Ordering::SeqCst));

    registry.stop_all().await;
    assert!(comp.stopped.load(Ordering::SeqCst));
}

// ─── 10. Error fluent builder across modules ────────────────────────────────

#[test]
fn error_fluent_builder_integration() {
    let err = AppError::not_found("user", Some("user-123"))
        .with_detail("search_field", "email")
        .with_detail("attempted_at", "2024-01-01");

    assert_eq!(err.code, ErrorCode::NotFound);
    assert_eq!(err.http_status, http::StatusCode::NOT_FOUND);
    assert!(!err.retryable);

    assert_eq!(err.details["search_field"], "email");
    assert_eq!(err.details["attempted_at"], "2024-01-01");
}

#[test]
fn error_retryability_across_codes() {
    let retryable = [
        ErrorCode::ServiceUnavailable,
        ErrorCode::ConnectionFailed,
        ErrorCode::Timeout,
        ErrorCode::RateLimited,
    ];
    for code in &retryable {
        assert!(code.is_retryable(), "{:?} should be retryable", code);
    }

    let non_retryable = [
        ErrorCode::NotFound,
        ErrorCode::Unauthorized,
        ErrorCode::Forbidden,
        ErrorCode::InvalidInput,
    ];
    for code in &non_retryable {
        assert!(!code.is_retryable(), "{:?} should NOT be retryable", code);
    }
}
