use rskit_component::Component;
use rskit_config::AppConfig;
use rskit_hook::Event;
use rskit_testutil::{FakeComponent, TestAppConfig, TestEvent};
use rskit_validation::Validate;

#[tokio::test]
async fn fake_component_tracks_lifecycle_and_health() {
    let component = FakeComponent::new("cache");

    assert_eq!(component.name(), "cache");
    assert_eq!(component.start_count(), 0);
    assert_eq!(component.stop_count(), 0);
    assert!(!component.health().is_healthy());

    component.start().await.unwrap();
    assert_eq!(component.start_count(), 1);
    assert!(component.health().is_healthy());

    component.stop().await.unwrap();
    assert_eq!(component.stop_count(), 1);
    assert!(!component.health().is_healthy());
}

#[test]
fn app_config_exposes_named_service_config() {
    let mut config = TestAppConfig::named("worker");
    config.apply_defaults();

    assert_eq!(config.service_config().name, "worker");
    assert_eq!(TestAppConfig::default().service_config().name, "service");
}

#[test]
fn app_config_validation_reports_service_field() {
    let config = TestAppConfig::named("");

    let err = config
        .validate()
        .expect_err("invalid embedded service config must fail");

    let field_errors = err.field_errors();
    assert!(field_errors.contains_key("service"));
    assert!(!field_errors.contains_key("name"));
}

#[test]
fn event_preserves_type_and_message() {
    let event = TestEvent::new("user.created", "created user 42");

    assert_eq!(event.event_type().as_str(), "user.created");
    assert_eq!(event.message, "created user 42");
    assert_eq!(event, event.clone());
}
