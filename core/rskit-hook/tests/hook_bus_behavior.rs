#![allow(missing_docs)]

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use rskit_hook::{
    CancellationToken, Event, EventBus, EventBusConfig, EventRegistry, EventType, HookError,
    HookRegistry,
};

#[derive(Debug)]
struct Ping(u32);
impl Event for Ping {
    fn event_type(&self) -> EventType {
        EventType::new("ping")
    }
}

#[derive(Debug)]
struct Pong;
impl Event for Pong {
    fn event_type(&self) -> EventType {
        EventType::new("pong")
    }
}

#[tokio::test]
async fn bounded_bus_publishes_reports_lag_and_formats_debug() {
    let bus = EventBus::<Ping>::new(EventBusConfig { capacity: 0 });
    let mut subscriber = bus.subscribe();
    assert!(format!("{bus:?}").contains("EventBus"));
    assert_eq!(bus.publish(Ping(1)).unwrap(), 1);
    assert_eq!(subscriber.recv().await.unwrap().0, 1);

    let mut lagging = bus.subscribe();
    bus.publish(Ping(2)).unwrap();
    bus.publish(Ping(3)).unwrap();
    assert_eq!(
        lagging.recv().await.unwrap_err().code(),
        rskit_errors::ErrorCode::RateLimited
    );
}

#[tokio::test]
async fn registry_publisher_and_subscriber_are_explicit() {
    let registry = EventRegistry::<Ping>::default();
    let mut subscriber = registry.register_subscriber();
    let publisher = registry.publisher();
    assert_eq!(publisher.publish(Ping(7)).unwrap(), 1);
    assert_eq!(subscriber.recv().await.unwrap().0, 7);
    assert!(format!("{registry:?}").contains("EventRegistry"));
    assert!(format!("{subscriber:?}").contains("Subscriber"));
}

#[test]
fn event_types_errors_and_registry_debug_are_human_readable() {
    let event_type = EventType::new("ping");
    assert_eq!(event_type.as_str(), "ping");
    assert_eq!(event_type.to_string(), "ping");
    let error = HookError::new("warn");
    assert_eq!(error.to_string(), "warn");

    let registry = HookRegistry::new();
    let _subscription = registry.on::<Ping>(EventType::new("ping"), |_, _| Ok(()));
    let formatted = format!("{registry:?}");
    assert!(formatted.contains("ping"));
}

#[test]
fn hook_registry_preserves_order_errors_and_snapshot_semantics() {
    let registry = Arc::new(HookRegistry::new());
    let count = Arc::new(AtomicUsize::new(0));
    let first = Arc::clone(&count);
    let second = Arc::clone(&count);
    let registry_for_handler = Arc::clone(&registry);
    let _u1 = registry.on::<Ping>(EventType::new("ping"), move |_, event| {
        assert_eq!(event.0, 9);
        first.fetch_add(1, Ordering::SeqCst);
        registry_for_handler.clear::<Ping>(&EventType::new("ping"));
        Err(HookError::new("non fatal"))
    });
    let _u2 = registry.on::<Ping>(EventType::new("ping"), move |_, _| {
        second.fetch_add(1, Ordering::SeqCst);
        Ok(())
    });

    assert!(registry.has_handlers::<Ping>(&EventType::new("ping")));
    let error = registry
        .emit(&Ping(9), CancellationToken::new())
        .unwrap_err();
    assert_eq!(error.message(), "non fatal");
    assert_eq!(count.load(Ordering::SeqCst), 2);
    assert!(!registry.has_handlers::<Ping>(&EventType::new("ping")));
}

#[test]
fn fatal_cancelled_and_panicking_handlers_are_reported() {
    let registry = HookRegistry::new();
    let count = Arc::new(AtomicUsize::new(0));
    let after_fatal = Arc::clone(&count);
    let _u1 = registry.on::<Pong>(EventType::new("pong"), |_, _| Err(HookError::fatal("stop")));
    let _u2 = registry.on::<Pong>(EventType::new("pong"), move |_, _| {
        after_fatal.fetch_add(1, Ordering::SeqCst);
        Ok(())
    });
    assert!(
        registry
            .emit(&Pong, CancellationToken::new())
            .unwrap_err()
            .is_fatal()
    );
    assert_eq!(count.load(Ordering::SeqCst), 0);

    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert!(registry.emit(&Pong, cancelled).unwrap_err().is_fatal());

    let panics = HookRegistry::new();
    let _panic = panics.on::<Ping>(EventType::new("ping"), |_, _| panic!("boom"));
    assert!(
        panics
            .emit(&Ping(1), CancellationToken::new())
            .unwrap_err()
            .message()
            .contains("boom")
    );
}
