#![allow(missing_docs)]

use rskit::{
    AppError, AppResult, ErrorCode, Handler, Health, HealthStatus, Pool, PoolConfig, RateLimiter,
    RetryPolicy,
};
use rskit_worker::Event;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[test]
fn error_code_accessible_from_facade() {
    let error = AppError::new(ErrorCode::NotFound, "not found");
    assert_eq!(error.code(), ErrorCode::NotFound);
    assert!(!error.is_retryable());
}

#[test]
fn retry_policy_accessible_from_facade() {
    let policy = RetryPolicy::new().with_max_attempts(2);
    assert_eq!(policy.max_attempts, 2);
}

#[test]
fn rate_limiter_accessible_from_facade() {
    let limiter = RateLimiter::new("facade-rl", 10, 5).unwrap();
    assert!(limiter.check().is_ok());
}

#[tokio::test]
async fn pool_accessible_from_facade() {
    struct EchoHandler;

    #[async_trait::async_trait]
    impl Handler<i32, i32> for EchoHandler {
        async fn handle(
            &self,
            task: i32,
            _emit: mpsc::Sender<Event<i32>>,
            _cancel: CancellationToken,
        ) -> AppResult<i32> {
            Ok(task)
        }
    }

    let pool = Pool::new(Arc::new(EchoHandler), PoolConfig::new("facade-pool"));
    let handle = pool.submit(99).await.unwrap();
    assert_eq!(handle.result().await.unwrap(), 99);
}

#[test]
fn health_types_accessible_from_facade() {
    let health = Health::healthy("svc");
    assert_eq!(health.status, HealthStatus::Healthy);
    assert!(health.is_healthy());
}

#[cfg(feature = "version")]
#[test]
fn version_crate_accessible_from_facade() {
    let _ = std::any::TypeId::of::<rskit::version::VersionInfo>();
}

#[cfg(feature = "schema")]
#[test]
fn schema_crate_accessible_from_facade() {
    let _ = std::any::TypeId::of::<rskit::schema::ValidationLimits>();
}

#[cfg(feature = "hook")]
#[test]
fn hook_crate_accessible_from_facade() {
    let _ = std::any::TypeId::of::<rskit::hook::HookRegistry>();
}
