#![allow(missing_docs)]

use rskit::resilience::{CbConfig, CircuitBreaker, RetryPolicy};
/// Facade integration tests — verify that types from all sub-crates
/// are accessible through the `rskit` facade.
use rskit::{AppError, ErrorCode};
use rskit::{Health, HealthStatus};

#[test]
fn errors_accessible_via_facade() {
    let e = AppError::not_found("item", Some("1"));
    assert_eq!(e.code(), ErrorCode::NotFound);
}

#[test]
fn health_accessible_via_facade() {
    let h = Health::healthy("svc");
    assert_eq!(h.status, HealthStatus::Healthy);
}

#[test]
fn resilience_accessible_via_facade() {
    let _cb = CircuitBreaker::new(CbConfig::new("test")).unwrap();
    let _rp = RetryPolicy::new().with_max_attempts(2);
}
