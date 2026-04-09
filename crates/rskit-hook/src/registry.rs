//! Thread-safe registry for hook handlers.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use parking_lot::RwLock;

use crate::types::{Action, Event, EventType, HookHandler, HookResult};

type HandlerMap = Arc<RwLock<HashMap<EventType, Vec<(usize, HookHandler)>>>>;

/// A thread-safe registry that maps [`EventType`]s to ordered handler lists.
///
/// Handlers are executed in registration order. The first handler that returns
/// [`Action::Abort`] short-circuits evaluation; [`Action::Modify`] is noted and
/// execution continues so later handlers can further modify. If no handler
/// aborts, the last `Modify` result (or `Continue`) is returned.
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
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(RwLock::new(HashMap::new())),
            next_id: AtomicUsize::new(0),
        }
    }

    /// Register a handler for the given event type.
    ///
    /// Returns a closure that, when called, removes this handler from the
    /// registry (i.e. an "unsubscribe" handle).
    pub fn on(
        &self,
        event_type: EventType,
        handler: impl Fn(&dyn Event) -> HookResult + Send + Sync + 'static,
    ) -> Box<dyn FnOnce() + Send> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        self.handlers
            .write()
            .entry(event_type.clone())
            .or_default()
            .push((id, Box::new(handler)));

        let handlers = self.handlers.clone();
        Box::new(move || {
            let mut map = handlers.write();
            if let Some(vec) = map.get_mut(&event_type) {
                vec.retain(|(handler_id, _)| *handler_id != id);
            }
        })
    }

    /// Emit an event to all registered handlers for its type.
    ///
    /// Returns the combined result: the first `Abort` wins; otherwise the last
    /// `Modify` is returned; if none, `Continue`.
    pub fn emit(&self, event: &dyn Event) -> HookResult {
        let event_type = event.event_type();
        let handlers = self.handlers.read();

        let Some(handler_list) = handlers.get(&event_type) else {
            return HookResult::ok();
        };

        let mut last_modify: Option<HookResult> = None;

        for (_, handler) in handler_list {
            let result = handler(event);
            match result.action {
                Action::Abort => return result,
                Action::Modify => {
                    last_modify = Some(result);
                }
                Action::Continue => {}
            }
        }

        last_modify.unwrap_or_else(HookResult::ok)
    }

    /// Check whether any handlers are registered for the given event type.
    pub fn has_handlers(&self, event_type: &EventType) -> bool {
        let handlers = self.handlers.read();
        handlers.get(event_type).is_some_and(|vec| !vec.is_empty())
    }

    /// Remove all handlers for the specified event types.
    pub fn clear(&self, event_types: &[EventType]) {
        let mut handlers = self.handlers.write();
        for et in event_types {
            handlers.remove(et);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::Any;
    use std::sync::atomic::AtomicU32;

    // ── Test-local event types ──────────────────────────────────────────

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

    // ── Tests ───────────────────────────────────────────────────────────

    #[test]
    fn test_registry_new_empty() {
        let reg = HookRegistry::new();
        assert!(!reg.has_handlers(&ping_type()));
        assert!(!reg.has_handlers(&pong_type()));
    }

    #[test]
    fn test_register_and_emit() {
        let reg = HookRegistry::new();
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let _unsub = reg.on(ping_type(), move |_event| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            HookResult::ok()
        });

        assert!(reg.has_handlers(&ping_type()));
        assert!(!reg.has_handlers(&pong_type()));

        let result = reg.emit(&Ping);
        assert_eq!(result.action, Action::Continue);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_emit_no_handlers() {
        let reg = HookRegistry::new();
        let result = reg.emit(&Pong);
        assert_eq!(result.action, Action::Continue);
    }

    #[test]
    fn test_emit_abort_short_circuits() {
        let reg = HookRegistry::new();
        let counter = Arc::new(AtomicU32::new(0));

        let c1 = counter.clone();
        let _unsub1 = reg.on(ping_type(), move |_| {
            c1.fetch_add(1, Ordering::SeqCst);
            HookResult::abort("blocked")
        });

        let c2 = counter.clone();
        let _unsub2 = reg.on(ping_type(), move |_| {
            c2.fetch_add(1, Ordering::SeqCst);
            HookResult::ok()
        });

        let result = reg.emit(&Ping);
        assert_eq!(result.action, Action::Abort);
        assert_eq!(result.reason, "blocked");
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_emit_modify_continues() {
        let reg = HookRegistry::new();

        let _unsub1 = reg.on(ping_type(), |_| HookResult::modify(0.5_f64, "lower temp"));

        let _unsub2 = reg.on(ping_type(), |_| HookResult::modify(0.3_f64, "even lower"));

        let result = reg.emit(&Ping);
        assert_eq!(result.action, Action::Modify);
        assert_eq!(result.reason, "even lower");
    }

    #[test]
    fn test_unsubscribe() {
        let reg = HookRegistry::new();
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let error_type = EventType::new("error");
        let unsub = reg.on(error_type.clone(), move |_| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            HookResult::ok()
        });

        assert!(reg.has_handlers(&error_type));

        unsub();

        assert!(!reg.has_handlers(&error_type));

        // Define a simple error event for the emit call
        struct ErrorEvent;
        impl Event for ErrorEvent {
            fn event_type(&self) -> EventType {
                EventType::new("error")
            }
            fn as_any(&self) -> &dyn Any {
                self
            }
        }

        reg.emit(&ErrorEvent);
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_clear() {
        let reg = HookRegistry::new();

        let a = EventType::new("a");
        let b = EventType::new("b");
        let c = EventType::new("c");

        let _unsub1 = reg.on(a.clone(), |_| HookResult::ok());
        let _unsub2 = reg.on(b.clone(), |_| HookResult::ok());
        let _unsub3 = reg.on(c.clone(), |_| HookResult::ok());

        assert!(reg.has_handlers(&a));
        assert!(reg.has_handlers(&b));
        assert!(reg.has_handlers(&c));

        reg.clear(&[a.clone(), b.clone()]);

        assert!(!reg.has_handlers(&a));
        assert!(!reg.has_handlers(&b));
        assert!(reg.has_handlers(&c));
    }

    #[test]
    fn test_multiple_handlers_same_event() {
        let reg = HookRegistry::new();
        let counter = Arc::new(AtomicU32::new(0));

        for _ in 0..3 {
            let c = counter.clone();
            let _unsub = reg.on(pong_type(), move |_| {
                c.fetch_add(1, Ordering::SeqCst);
                HookResult::ok()
            });
        }

        reg.emit(&Pong);
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn test_debug_format() {
        let reg = HookRegistry::new();
        let _unsub = reg.on(ping_type(), |_| HookResult::ok());
        let debug = format!("{reg:?}");
        assert!(debug.contains("HookRegistry"));
    }

    #[test]
    fn test_default() {
        let reg = HookRegistry::default();
        assert!(!reg.has_handlers(&ping_type()));
    }
}
