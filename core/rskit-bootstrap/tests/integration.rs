use rskit_bootstrap::component::Component;
use rskit_bootstrap::{Health, HealthStatus, Registry};
use rskit_errors::AppResult;
use std::sync::Arc;

struct NoopComponent;

#[async_trait::async_trait]
impl Component for NoopComponent {
    fn name(&self) -> &str {
        "noop"
    }
    async fn start(&self) -> AppResult<()> {
        Ok(())
    }
    async fn stop(&self) -> AppResult<()> {
        Ok(())
    }
    fn health(&self) -> Health {
        Health::healthy("noop")
    }
}

#[test]
fn health_states_are_correct() {
    assert_eq!(Health::healthy("x").status, HealthStatus::Healthy);
    assert_eq!(Health::degraded("x", "slow").status, HealthStatus::Degraded);
    assert_eq!(
        Health::unhealthy("x", "down").status,
        HealthStatus::Unhealthy
    );
}

#[tokio::test]
async fn registry_starts_and_stops() {
    let mut reg = Registry::new();
    reg.register(Arc::new(NoopComponent));
    reg.start_all().await.unwrap();
    reg.stop_all().await.unwrap();
}
