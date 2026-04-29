use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use parking_lot::RwLock;
use tokio_util::sync::CancellationToken;

use crate::types::{Action, Event, EventType, HookHandler, HookResult};

type HandlerMap = Arc<RwLock<HashMap<EventType, Vec<(usize, HookHandler)>>>>;

#[derive(Debug)]
struct HookDispatchError(String);

impl HookDispatchError {
    fn cancelled() -> Self {
        Self("hook dispatch cancelled".to_string())
    }

    fn panicked(payload: Box<dyn std::any::Any + Send>) -> Self {
        let message = if let Some(message) = payload.downcast_ref::<&str>() {
            (*message).to_string()
        } else if let Some(message) = payload.downcast_ref::<String>() {
            message.clone()
        } else {
            "unknown panic payload".to_string()
        };
        Self(format!("hook handler panicked: {message}"))
    }
}

impl std::fmt::Display for HookDispatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for HookDispatchError {}

/// A thread-safe registry that maps [`EventType`]s to ordered handler lists.
///
/// Handlers are executed in registration order. The first handler that returns
/// [`Action::Abort`] short-circuits evaluation; [`Action::Modify`] is noted and
/// execution continues so later handlers can further modify. If no handler
/// aborts, the last `Modify` result or the last non-fatal error result wins.
pub struct HookRegistry {
    handlers: HandlerMap,
    next_id: AtomicUsize,
}

impl std::fmt::Debug for HookRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let handlers = self.handlers.read();
        let counts: HashMap<&EventType, usize> = handlers
            .iter()
            .map(|(event_type, items)| (event_type, items.len()))
            .collect();
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
            let mut map = handlers.write();
            if let Some(items) = map.get_mut(&event_type) {
                items.retain(|(handler_id, _)| *handler_id != id);
            }
        })
    }

    /// Emit an event to all registered handlers for its type.
    #[must_use]
    pub fn emit(&self, event: &dyn Event, cancel: CancellationToken) -> HookResult {
        let event_type = event.event_type();
        let handlers = self.handlers.read();

        let Some(handler_list) = handlers.get(&event_type) else {
            return HookResult::ok();
        };

        let mut last_result: Option<HookResult> = None;

        for (_, handler) in handler_list {
            if cancel.is_cancelled() {
                return HookResult::abort_with_error(HookDispatchError::cancelled());
            }

            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                handler(cancel.clone(), event)
            }))
            .unwrap_or_else(|payload| {
                HookResult::continue_with_error(HookDispatchError::panicked(payload))
            });

            match result.action {
                Action::Abort => return result,
                Action::Modify => last_result = Some(result),
                Action::Continue => {
                    if result.error.is_some() {
                        last_result = Some(result);
                    }
                }
            }
        }

        last_result.unwrap_or_else(HookResult::ok)
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

#[cfg(test)]
mod tests {
    use std::any::Any;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    use tokio_util::sync::CancellationToken;

    use super::HookRegistry;
    use crate::{Action, Event, EventType, HookResult};

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
        let counter_clone = Arc::clone(&counter);

        let _unsubscribe = registry.on(ping_type(), move |_, _| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            HookResult::ok()
        });

        let result = registry.emit(&Ping, CancellationToken::new());
        assert_eq!(result.action, Action::Continue);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn emit_no_handlers_returns_continue() {
        let registry = HookRegistry::new();
        let result = registry.emit(&Pong, CancellationToken::new());
        assert_eq!(result.action, Action::Continue);
    }

    #[test]
    fn emit_abort_short_circuits() {
        let registry = HookRegistry::new();
        let counter = Arc::new(AtomicU32::new(0));

        let first = Arc::clone(&counter);
        let _unsubscribe1 = registry.on(ping_type(), move |_, _| {
            first.fetch_add(1, Ordering::SeqCst);
            HookResult::abort("blocked")
        });

        let second = Arc::clone(&counter);
        let _unsubscribe2 = registry.on(ping_type(), move |_, _| {
            second.fetch_add(1, Ordering::SeqCst);
            HookResult::ok()
        });

        let result = registry.emit(&Ping, CancellationToken::new());
        assert_eq!(result.action, Action::Abort);
        assert_eq!(result.reason, "blocked");
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn emit_modify_returns_last_modify() {
        let registry = HookRegistry::new();
        let _unsubscribe1 = registry.on(ping_type(), |_, _| HookResult::modify(0.5_f64, "lower"));
        let _unsubscribe2 = registry.on(ping_type(), |_, _| HookResult::modify(0.3_f64, "lowest"));

        let result = registry.emit(&Ping, CancellationToken::new());
        assert_eq!(result.action, Action::Modify);
        assert_eq!(result.reason, "lowest");
    }

    #[test]
    fn cancelled_emit_aborts_before_dispatch() {
        let registry = HookRegistry::new();
        let counter = Arc::new(AtomicU32::new(0));
        let cloned = Arc::clone(&counter);
        let _unsubscribe = registry.on(ping_type(), move |_, _| {
            cloned.fetch_add(1, Ordering::SeqCst);
            HookResult::ok()
        });

        let token = CancellationToken::new();
        token.cancel();
        let result = registry.emit(&Ping, token);

        assert_eq!(result.action, Action::Abort);
        assert!(result.error.is_some());
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn panicking_handler_becomes_continue_with_error() {
        let registry = HookRegistry::new();
        let counter = Arc::new(AtomicU32::new(0));

        let _unsubscribe1 = registry.on(ping_type(), |_, _| panic!("boom"));
        let cloned = Arc::clone(&counter);
        let _unsubscribe2 = registry.on(ping_type(), move |_, _| {
            cloned.fetch_add(1, Ordering::SeqCst);
            HookResult::ok()
        });

        let result = registry.emit(&Ping, CancellationToken::new());

        assert_eq!(result.action, Action::Continue);
        assert!(result.error.is_some());
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn unsubscribe_removes_handler() {
        let registry = HookRegistry::new();
        let counter = Arc::new(AtomicU32::new(0));
        let cloned = Arc::clone(&counter);

        let event_type = EventType::new("error");
        let unsubscribe = registry.on(event_type.clone(), move |_, _| {
            cloned.fetch_add(1, Ordering::SeqCst);
            HookResult::ok()
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

        let _unsubscribe1 = registry.on(a.clone(), |_, _| HookResult::ok());
        let _unsubscribe2 = registry.on(b.clone(), |_, _| HookResult::ok());
        let _unsubscribe3 = registry.on(c.clone(), |_, _| HookResult::ok());

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
                HookResult::ok()
            });
        }

        let _ = registry.emit(&Pong, CancellationToken::new());
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }
}
