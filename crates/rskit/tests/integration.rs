/// Facade integration tests — verify that types from all sub-crates
/// are accessible through the `rskit` facade.
use rskit::{AppError, AppResult, ErrorCode};
use rskit::{Health, HealthStatus};
use rskit::resilience::{RetryPolicy, CircuitBreaker, CbConfig};

#[test]
fn errors_accessible_via_facade() {
    let e: AppResult<()> = Err(AppError::not_found("item", "1"));
    assert_eq!(e.unwrap_err().code, ErrorCode::NotFound);
}

#[test]
fn health_accessible_via_facade() {
    let h = Health::healthy("svc");
    assert_eq!(h.status, HealthStatus::Healthy);
}

#[test]
fn resilience_accessible_via_facade() {
    let _cb = CircuitBreaker::new(CbConfig::new("test"));
    let _rp = RetryPolicy::new().with_max_attempts(2);
}
