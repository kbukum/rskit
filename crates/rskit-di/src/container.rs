use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use parking_lot::RwLock;
use rskit_errors::{AppError, AppResult};

// ── Registration kinds ────────────────────────────────────────────────────────

type ArcAny = Arc<dyn Any + Send + Sync>;
type Factory = Arc<dyn Fn() -> AppResult<ArcAny> + Send + Sync>;

enum Registration {
    Eager(ArcAny),
    Lazy(Factory),
    Singleton {
        factory: Factory,
        instance: OnceLock<ArcAny>,
    },
}

// ── Closeable ─────────────────────────────────────────────────────────────────

/// Trait implemented by registered values that need async cleanup.
#[async_trait]
pub trait Closeable: Send + Sync {
    /// Release resources held by this value.
    async fn close(&self) -> AppResult<()>;
}

// ── Container ─────────────────────────────────────────────────────────────────

/// Thread-safe runtime dependency injection container.
///
/// Each dependency is keyed by its concrete Rust type (`TypeId`).  Three
/// registration modes are supported:
///
/// | Mode | Fn | Description |
/// |------|----|-|
/// | Eager | [`register`](Self::register) | Pre-built value, returned as-is on every resolve |
/// | Lazy factory | [`register_factory`](Self::register_factory) | Called fresh on every resolve |
/// | Singleton | [`register_singleton`](Self::register_singleton) | Called once; result cached |
pub struct Container {
    registrations: RwLock<HashMap<TypeId, Registration>>,
    closeables: RwLock<Vec<Arc<dyn Closeable>>>,
}

impl Default for Container {
    fn default() -> Self {
        Self::new()
    }
}

impl Container {
    /// Create an empty container.
    pub fn new() -> Self {
        Self {
            registrations: RwLock::new(HashMap::new()),
            closeables: RwLock::new(Vec::new()),
        }
    }

    /// Register a pre-built value (equivalent to gokit `RegisterEager`).
    pub fn register<T: Send + Sync + 'static>(&self, value: Arc<T>) {
        let any: ArcAny = value.clone();
        self.registrations
            .write()
            .insert(TypeId::of::<T>(), Registration::Eager(any));
    }

    /// Register a factory called fresh on every resolve (equivalent to `RegisterLazy`).
    pub fn register_factory<T, F>(&self, factory: F)
    where
        T: Send + Sync + 'static,
        F: Fn() -> AppResult<Arc<T>> + Send + Sync + 'static,
    {
        let f: Factory = Arc::new(move || factory().map(|v| v as ArcAny));
        self.registrations
            .write()
            .insert(TypeId::of::<T>(), Registration::Lazy(f));
    }

    /// Register a singleton factory — called once, result cached (equivalent to `RegisterSingleton`).
    pub fn register_singleton<T, F>(&self, factory: F)
    where
        T: Send + Sync + 'static,
        F: Fn() -> AppResult<Arc<T>> + Send + Sync + 'static,
    {
        let f: Factory = Arc::new(move || factory().map(|v| v as ArcAny));
        self.registrations.write().insert(
            TypeId::of::<T>(),
            Registration::Singleton {
                factory: f,
                instance: OnceLock::new(),
            },
        );
    }

    /// Resolve a registered type, returning `Err(NotFound)` if not registered.
    pub fn resolve<T: Send + Sync + 'static>(&self) -> AppResult<Arc<T>> {
        let arc_any = {
            let guard = self.registrations.read();
            match guard.get(&TypeId::of::<T>()) {
                None => {
                    return Err(AppError::not_found(std::any::type_name::<T>(), None));
                }
                Some(Registration::Eager(v)) => v.clone(),
                Some(Registration::Lazy(f)) => f()?,
                Some(Registration::Singleton { factory, instance }) => {
                    // OnceLock::get_or_try_init would be ideal but requires nightly;
                    // use a two-phase approach for stable Rust.
                    if let Some(v) = instance.get() {
                        v.clone()
                    } else {
                        let v = factory()?;
                        let _ = instance.set(v.clone());
                        instance.get().unwrap().clone()
                    }
                }
            }
        };
        arc_any.downcast::<T>().map_err(|_| {
            AppError::new(
                rskit_errors::ErrorCode::Internal,
                "type downcast failed in DI container",
            )
        })
    }

    /// Returns `true` if `T` has been registered.
    pub fn is_registered<T: 'static>(&self) -> bool {
        self.registrations.read().contains_key(&TypeId::of::<T>())
    }

    /// Call [`Closeable::close`] on all registered closeable values.
    pub async fn close(&self) -> AppResult<()> {
        let closeables = self.closeables.read().clone();
        for c in closeables {
            c.close().await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Svc {
        val: i32,
    }

    #[test]
    fn register_and_resolve_eager() {
        let c = Container::new();
        c.register(Arc::new(Svc { val: 42 }));
        let s = c.resolve::<Svc>().unwrap();
        assert_eq!(s.val, 42);
    }

    #[test]
    fn resolve_unregistered_returns_not_found() {
        let c = Container::new();
        let r = c.resolve::<Svc>();
        assert!(r.is_err());
    }

    #[test]
    fn register_factory_called_on_each_resolve() {
        use std::sync::atomic::{AtomicI32, Ordering};
        static COUNTER: AtomicI32 = AtomicI32::new(0);

        let c = Container::new();
        c.register_factory(|| {
            let n = COUNTER.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(Svc { val: n }))
        });

        let s1 = c.resolve::<Svc>().unwrap();
        let s2 = c.resolve::<Svc>().unwrap();
        assert_ne!(s1.val, s2.val);
    }

    #[test]
    fn register_singleton_returns_same_instance() {
        use std::sync::atomic::{AtomicI32, Ordering};
        static CTR: AtomicI32 = AtomicI32::new(0);

        let c = Container::new();
        c.register_singleton(|| {
            let n = CTR.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(Svc { val: n }))
        });

        let s1 = c.resolve::<Svc>().unwrap();
        let s2 = c.resolve::<Svc>().unwrap();
        assert_eq!(s1.val, s2.val);
        assert_eq!(CTR.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn is_registered_reflects_state() {
        let c = Container::new();
        assert!(!c.is_registered::<Svc>());
        c.register(Arc::new(Svc { val: 0 }));
        assert!(c.is_registered::<Svc>());
    }
}
