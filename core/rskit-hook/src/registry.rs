use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use parking_lot::RwLock;
use tokio_util::sync::CancellationToken;

use crate::types::{Event, EventType, HookError, HookResult};

type ErasedEvent = dyn Any + Send + Sync;
type ErasedHandler = Arc<dyn Fn(CancellationToken, &ErasedEvent) -> HookResult + Send + Sync>;
type HandlerKey = (TypeId, EventType);
type HandlerMap = Arc<RwLock<HashMap<HandlerKey, Vec<(usize, ErasedHandler)>>>>;

/// A thread-safe registry that maps typed events to ordered observe-only handlers.
///
/// Registration and emission are constructor-injected through registry values;
/// there is no global hook registry. Handlers for the same event type run in registration order.
/// A fatal [`HookError`] short-circuits the remaining handlers for that event type;
/// non-fatal errors are collected and the first non-fatal error is returned after all handlers run.
pub struct HookRegistry {
    handlers: HandlerMap,
    event_types: Arc<RwLock<HashMap<TypeId, EventType>>>,
    next_id: AtomicUsize,
}

impl std::fmt::Debug for HookRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let event_types = self.event_types.read();
        let handlers = self.handlers.read();
        let counts = handlers
            .iter()
            .map(|((type_id, registered_type), handlers)| {
                let event_type = event_types
                    .get(type_id)
                    .unwrap_or(registered_type)
                    .to_string();
                (event_type, handlers.len())
            })
            .collect::<HashMap<_, _>>();
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
            event_types: Arc::new(RwLock::new(HashMap::new())),
            next_id: AtomicUsize::new(0),
        }
    }

    /// Register a typed handler for event `E`.
    ///
    /// The returned function unregisters this handler when invoked.
    pub fn on<E>(
        &self,
        event_type: EventType,
        handler: impl Fn(CancellationToken, &E) -> HookResult + Send + Sync + 'static,
    ) -> Box<dyn FnOnce() + Send>
    where
        E: Event,
    {
        let type_id = TypeId::of::<E>();
        let key = (type_id, event_type.clone());
        self.event_types.write().insert(type_id, event_type);
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        self.handlers.write().entry(key.clone()).or_default().push((
            id,
            Arc::new(move |cancel, event| {
                let Some(event) = event.downcast_ref::<E>() else {
                    return Err(HookError::fatal("hook event type mismatch"));
                };
                handler(cancel, event)
            }),
        ));
        let handlers = Arc::clone(&self.handlers);
        Box::new(move || {
            if let Some(items) = handlers.write().get_mut(&key) {
                items.retain(|(handler_id, _)| *handler_id != id);
            }
        })
    }

    /// Emit an event to all registered handlers for its concrete type.
    ///
    /// Dispatch uses snapshot semantics: handlers registered
    /// or removed during an in-flight emit do not affect that emit, but apply to subsequent emits.
    pub fn emit<E>(&self, event: &E, cancel: CancellationToken) -> HookResult
    where
        E: Event,
    {
        let key = (TypeId::of::<E>(), event.event_type());
        let handler_list = {
            let handlers = self.handlers.read();
            let Some(handler_list) = handlers.get(&key) else {
                return Ok(());
            };
            handler_list
                .iter()
                .map(|(_, handler)| Arc::clone(handler))
                .collect::<Vec<_>>()
        };
        let mut first_error: Option<HookError> = None;
        for handler in handler_list {
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

    /// Check whether any handlers are registered for event `E` and `event_type`.
    #[must_use]
    pub fn has_handlers<E>(&self, event_type: &EventType) -> bool
    where
        E: Event,
    {
        self.handlers
            .read()
            .get(&(TypeId::of::<E>(), event_type.clone()))
            .is_some_and(|handlers| !handlers.is_empty())
    }

    /// Remove all handlers for event `E` and `event_type`.
    pub fn clear<E>(&self, event_type: &EventType)
    where
        E: Event,
    {
        self.handlers
            .write()
            .remove(&(TypeId::of::<E>(), event_type.clone()));
    }

    /// Remove all handlers from the registry.
    pub fn clear_all(&self) {
        let mut handlers = self.handlers.write();
        handlers.clear();
    }
}

/// Typed lifecycle hook registry.
///
/// Lifecycle hooks use the same ordered synchronous dispatch semantics as [`HookRegistry`];
/// the alias exists to make lifecycle-specific injection points self-documenting.
pub type LifecycleHookRegistry = HookRegistry;

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
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    use tokio_util::sync::CancellationToken;

    use super::{HookRegistry, LifecycleHookRegistry, panic_message};
    use crate::{Event, EventType, HookError};

    struct Ping;
    impl Event for Ping {
        fn event_type(&self) -> EventType {
            EventType::new("ping")
        }
    }
    struct Pong;
    impl Event for Pong {
        fn event_type(&self) -> EventType {
            EventType::new("pong")
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
        let _unsubscribe = registry.on::<Ping>(ping_type(), move |_, _| {
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
        let _unsubscribe1 = registry.on::<Ping>(ping_type(), |_, _| Err(HookError::new("warn")));
        let cloned = Arc::clone(&counter);
        let _unsubscribe2 = registry.on::<Ping>(ping_type(), move |_, _| {
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
        let _unsubscribe1 =
            registry.on::<Ping>(ping_type(), |_, _| Err(HookError::fatal("blocked")));
        let _unsubscribe2 =
            registry.on::<Ping>(ping_type(), |_, _| unreachable!("short-circuited"));
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
        let _unsubscribe = registry.on::<Ping>(ping_type(), |_, _| unreachable!("cancelled"));
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
        let _unsubscribe1 = registry.on::<Ping>(ping_type(), |_, _| panic!("boom"));
        let cloned = Arc::clone(&counter);
        let _unsubscribe2 = registry.on::<Ping>(ping_type(), move |_, _| {
            cloned.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });
        assert!(registry.emit(&Ping, CancellationToken::new()).is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn unsubscribe_removes_handler() {
        let registry = HookRegistry::new();
        struct ErrorEvent;
        impl Event for ErrorEvent {
            fn event_type(&self) -> EventType {
                EventType::new("error")
            }
        }
        let unsubscribe =
            registry.on::<ErrorEvent>(EventType::new("error"), |_, _| unreachable!("unsubscribed"));
        unsubscribe();
        registry
            .emit(&ErrorEvent, CancellationToken::new())
            .unwrap();
    }

    #[test]
    fn clear_removes_selected_event_types() {
        let registry = HookRegistry::new();
        struct A;
        impl Event for A {
            fn event_type(&self) -> EventType {
                EventType::new("a")
            }
        }
        struct B;
        impl Event for B {
            fn event_type(&self) -> EventType {
                EventType::new("b")
            }
        }
        let a = EventType::new("a");
        let b = EventType::new("b");
        let _u1 = registry.on::<A>(a.clone(), |_, _| Ok(()));
        let _u2 = registry.on::<B>(b.clone(), |_, _| Ok(()));
        registry.clear::<A>(&a);
        assert!(!registry.has_handlers::<A>(&a));
        assert!(registry.has_handlers::<B>(&b));
        registry.clear_all();
        assert!(!registry.has_handlers::<B>(&b));
    }

    #[test]
    fn multiple_handlers_same_event_all_run() {
        let registry = HookRegistry::new();
        let counter = Arc::new(AtomicU32::new(0));
        for _ in 0..3 {
            let cloned = Arc::clone(&counter);
            let _unsubscribe = registry.on::<Pong>(pong_type(), move |_, _| {
                cloned.fetch_add(1, Ordering::SeqCst);
                Ok(())
            });
        }
        let _ = registry.emit(&Pong, CancellationToken::new());
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn handler_can_mutate_registry_during_emit() {
        let registry = Arc::new(HookRegistry::new());
        let counter = Arc::new(AtomicU32::new(0));

        let registry_for_handler = Arc::clone(&registry);
        let _unsubscribe1 = registry.on::<Ping>(ping_type(), move |_, _| {
            registry_for_handler.clear::<Ping>(&ping_type());
            Ok(())
        });

        let cloned = Arc::clone(&counter);
        let _unsubscribe2 = registry.on::<Ping>(ping_type(), move |_, _| {
            cloned.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });

        registry
            .emit(&Ping, CancellationToken::new())
            .expect("emit");

        assert_eq!(counter.load(Ordering::SeqCst), 1);
        assert!(!registry.has_handlers::<Ping>(&ping_type()));
    }

    #[test]
    fn default_and_lifecycle_registry_are_empty() {
        let registry = LifecycleHookRegistry::default();

        assert!(!registry.has_handlers::<Ping>(&ping_type()));
        assert!(format!("{registry:?}").contains("handler_counts"));
    }

    #[test]
    fn debug_reports_registered_event_type_counts() {
        let registry = HookRegistry::new();
        let _unsubscribe = registry.on::<Ping>(ping_type(), |_, _| Ok(()));

        let debug = format!("{registry:?}");

        assert!(debug.contains("ping"));
        assert!(debug.contains('1'));
    }

    #[test]
    fn panic_message_handles_all_payload_shapes() {
        assert_eq!(panic_message(Box::new("borrowed")), "borrowed");
        assert_eq!(panic_message(Box::new(String::from("owned"))), "owned");
        assert_eq!(panic_message(Box::new(123_u32)), "unknown panic payload");
    }
}
