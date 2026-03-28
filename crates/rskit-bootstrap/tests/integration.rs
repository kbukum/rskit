use rskit_bootstrap::{Health, HealthStatus, Registry};
use rskit_bootstrap::component::Component;
use rskit_errors::AppResult;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

struct NoopComponent;

#[async_trait::async_trait]
impl Component for NoopComponent {
    async fn start(&self, _cancel: CancellationToken) -> AppResult<()> { Ok(()) }
    async fn stop(&self) -> AppResult<()> { Ok(()) }
    async fn health(&self) -> Health { Health::healthy("noop") }
}

#[test]
fn health_states_are_correct() {
    assert_eq!(Health::healthy("x").status, HealthStatus::Healthy);
    assert_eq!(Health::degraded("x", "slow").status, HealthStatus::Degraded);
    assert_eq!(Health::unhealthy("x", "down").status, HealthStatus::Unhealthy);
}

#[tokio::test]
async fn registry_starts_and_stops() {
    let mut reg = Registry::new();
    reg.register(Arc::new(NoopComponent));
    let cancel = CancellationToken::new();
    reg.start_all(cancel).await.unwrap();
    reg.stop_all().await.unwrap();
}
