use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use parking_lot::RwLock;
use tokio_util::sync::CancellationToken;

use crate::types::{Event, EventType, HookError, HookHandler, HookResult};

type HandlerMap = Arc<RwLock<HashMap<EventType, Vec<(usize, HookHandler)>>>>;

/// A thread-safe registry that maps [`EventType`]s to ordered observe-only handlers.
pub struct HookRegistry {
    handlers: HandlerMap,
    next_id: AtomicUsize,
}

impl std::fmt::Debug for HookRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let handlers = self.handlers.read();
        let counts: HashMap<&EventType, usize> =
            handlers.iter().map(|(k, v)| (k, v.len())).collect();
        f.debug_struct("HookRegistry")
            .field("handler_counts", &counts)
            .finish()
    }
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl HookRegistry {
    /// Create a new empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(RwLock::new(HashMap::new())),
            next_id: AtomicUsize::new(0),
        }
    }

    /// Register a handler for the given event type.
    pub fn on(
        &self,
        event_type: EventType,
        handler: impl Fn(CancellationToken, &dyn Event) -> HookResult + Send + Sync + 'static,
    ) -> Box<dyn FnOnce() + Send> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        self.handlers
            .write()
            .entry(event_type.clone())
            .or_default()
            .push((id, Box::new(handler)));
        let handlers = Arc::clone(&self.handlers);
        Box::new(move || {
            if let Some(items) = handlers.write().get_mut(&event_type) {
                items.retain(|(handler_id, _)| *handler_id != id);
            }
        })
    }

    /// Emit an event to all registered handlers for its type.
    pub fn emit(&self, event: &dyn Event, cancel: CancellationToken) -> HookResult {
        let event_type = event.event_type();
        let handlers = self.handlers.read();
        let Some(handler_list) = handlers.get(&event_type) else {
            return Ok(());
        };
        let mut first_error: Option<HookError> = None;
        for (_, handler) in handler_list {
            if cancel.is_cancelled() {
                let err = HookError::fatal("hook dispatch cancelled");
                return Err(err);
            }
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                handler(cancel.clone(), event)
            }))
            .unwrap_or_else(|payload| {
                Err(HookError::new(format!(
                    "hook handler panicked: {}",
                    panic_message(payload)
                )))
            });
            if let Err(err) = result {
                if err.is_fatal() {
                    return Err(err);
                }
                if first_error.is_none() {
                    first_error = Some(err);
                }
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    /// Check whether any handlers are registered for the given event type.
    #[must_use]
    pub fn has_handlers(&self, event_type: &EventType) -> bool {
        self.handlers
            .read()
            .get(event_type)
            .is_some_and(|handlers| !handlers.is_empty())
    }

    /// Remove all handlers for the specified event types.
    pub fn clear(&self, event_types: &[EventType]) {
        let mut handlers = self.handlers.write();
        for event_type in event_types {
            handlers.remove(event_type);
        }
    }
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::any::Any;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    use tokio_util::sync::CancellationToken;

    use super::HookRegistry;
    use crate::{Event, EventType, HookError};

    struct Ping;
    impl Event for Ping {
        fn event_type(&self) -> EventType {
            EventType::new("ping")
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    }
    struct Pong;
    impl Event for Pong {
        fn event_type(&self) -> EventType {
            EventType::new("pong")
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    }
    fn ping_type() -> EventType {
        EventType::new("ping")
    }
    fn pong_type() -> EventType {
        EventType::new("pong")
    }

    #[test]
    fn register_and_emit() {
        let registry = HookRegistry::new();
        let counter = Arc::new(AtomicU32::new(0));
        let cloned = Arc::clone(&counter);
        let _unsubscribe = registry.on(ping_type(), move |_, _| {
            cloned.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });
        assert!(registry.emit(&Ping, CancellationToken::new()).is_ok());
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn emit_no_handlers_returns_ok() {
        assert!(
            HookRegistry::new()
                .emit(&Pong, CancellationToken::new())
                .is_ok()
        );
    }

    #[test]
    fn non_fatal_error_does_not_short_circuit() {
        let registry = HookRegistry::new();
        let counter = Arc::new(AtomicU32::new(0));
        let _unsubscribe1 = registry.on(ping_type(), |_, _| Err(HookError::new("warn")));
        let cloned = Arc::clone(&counter);
        let _unsubscribe2 = registry.on(ping_type(), move |_, _| {
            cloned.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });
        let result = registry.emit(&Ping, CancellationToken::new());
        assert_eq!(result.expect_err("error").message(), "warn");
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn fatal_error_short_circuits() {
        let registry = HookRegistry::new();
        let counter = Arc::new(AtomicU32::new(0));
        let _unsubscribe1 = registry.on(ping_type(), |_, _| Err(HookError::fatal("blocked")));
        let cloned = Arc::clone(&counter);
        let _unsubscribe2 = registry.on(ping_type(), move |_, _| {
            cloned.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });
        assert!(
            registry
                .emit(&Ping, CancellationToken::new())
                .expect_err("fatal")
                .is_fatal()
        );
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn cancelled_emit_errors_before_dispatch() {
        let registry = HookRegistry::new();
        let counter = Arc::new(AtomicU32::new(0));
        let cloned = Arc::clone(&counter);
        let _unsubscribe = registry.on(ping_type(), move |_, _| {
            cloned.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });
        let token = CancellationToken::new();
        token.cancel();
        assert!(
            registry
                .emit(&Ping, token)
                .expect_err("cancelled")
                .is_fatal()
        );
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn panicking_handler_becomes_error_and_continues() {
        let registry = HookRegistry::new();
        let counter = Arc::new(AtomicU32::new(0));
        let _unsubscribe1 = registry.on(ping_type(), |_, _| panic!("boom"));
        let cloned = Arc::clone(&counter);
        let _unsubscribe2 = registry.on(ping_type(), move |_, _| {
            cloned.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });
        assert!(registry.emit(&Ping, CancellationToken::new()).is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn unsubscribe_removes_handler() {
        let registry = HookRegistry::new();
        let counter = Arc::new(AtomicU32::new(0));
        let cloned = Arc::clone(&counter);
        let unsubscribe = registry.on(EventType::new("error"), move |_, _| {
            cloned.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });
        unsubscribe();
        struct ErrorEvent;
        impl Event for ErrorEvent {
            fn event_type(&self) -> EventType {
                EventType::new("error")
            }
            fn as_any(&self) -> &dyn Any {
                self
            }
        }
        let _ = registry.emit(&ErrorEvent, CancellationToken::new());
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn clear_removes_selected_event_types() {
        let registry = HookRegistry::new();
        let a = EventType::new("a");
        let b = EventType::new("b");
        let c = EventType::new("c");
        let _u1 = registry.on(a.clone(), |_, _| Ok(()));
        let _u2 = registry.on(b.clone(), |_, _| Ok(()));
        let _u3 = registry.on(c.clone(), |_, _| Ok(()));
        registry.clear(&[a.clone(), b.clone()]);
        assert!(!registry.has_handlers(&a));
        assert!(!registry.has_handlers(&b));
        assert!(registry.has_handlers(&c));
    }

    #[test]
    fn multiple_handlers_same_event_all_run() {
        let registry = HookRegistry::new();
        let counter = Arc::new(AtomicU32::new(0));
        for _ in 0..3 {
            let cloned = Arc::clone(&counter);
            let _unsubscribe = registry.on(pong_type(), move |_, _| {
                cloned.fetch_add(1, Ordering::SeqCst);
                Ok(())
            });
        }
        let _ = registry.emit(&Pong, CancellationToken::new());
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }
}
