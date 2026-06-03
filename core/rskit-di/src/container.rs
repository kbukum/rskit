use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::{Mutex, RwLock};
use rskit_errors::{AppError, AppResult, ErrorCode};

type ArcAny = Arc<dyn Any + Send + Sync>;
type Factory = Arc<dyn Fn() -> AppResult<ArcAny> + Send + Sync>;

type CloseableArc = Arc<dyn Closeable>;

type SingletonFactory = Arc<dyn Fn() -> AppResult<SingletonValue> + Send + Sync>;

thread_local! {
    static RESOLUTION_STACK: RefCell<HashSet<TypeId>> = RefCell::new(HashSet::new());
}

#[derive(Clone)]
struct SingletonValue {
    any: ArcAny,
    closeable: Option<CloseableArc>,
}

struct SingletonRegistration {
    factory: SingletonFactory,
    value: Mutex<Option<SingletonValue>>,
}

impl SingletonRegistration {
    fn new(factory: SingletonFactory) -> Self {
        Self {
            factory,
            value: Mutex::new(None),
        }
    }

    fn resolve_any(&self) -> AppResult<ArcAny> {
        let mut guard = self.value.lock();
        if let Some(value) = guard.as_ref() {
            return Ok(Arc::clone(&value.any));
        }

        let value = (self.factory)()?;
        let any = Arc::clone(&value.any);
        *guard = Some(value);
        Ok(any)
    }

    fn resolve_closeable(&self) -> AppResult<Option<CloseableArc>> {
        let mut guard = self.value.lock();
        if let Some(value) = guard.as_ref() {
            return Ok(value.closeable.as_ref().map(Arc::clone));
        }

        let value = (self.factory)()?;
        let closeable = value.closeable.as_ref().map(Arc::clone);
        *guard = Some(value);
        Ok(closeable)
    }
}

#[derive(Clone)]
enum Registration {
    Eager(ArcAny),
    Lazy(Factory),
    Singleton(Arc<SingletonRegistration>),
}

enum CloseableRegistration {
    Eager(CloseableArc),
    Singleton(Arc<SingletonRegistration>),
}

struct ResolutionGuard {
    type_id: TypeId,
}

impl ResolutionGuard {
    fn enter<T: 'static>() -> AppResult<Self> {
        let type_id = TypeId::of::<T>();
        let type_name = std::any::type_name::<T>();
        let inserted = RESOLUTION_STACK.with(|stack| {
            let mut stack = stack.borrow_mut();
            if stack.contains(&type_id) {
                false
            } else {
                stack.insert(type_id)
            }
        });

        if inserted {
            Ok(Self { type_id })
        } else {
            Err(AppError::new(
                ErrorCode::Conflict,
                format!("circular dependency detected while resolving {type_name}"),
            ))
        }
    }
}

impl Drop for ResolutionGuard {
    fn drop(&mut self) {
        RESOLUTION_STACK.with(|stack| {
            stack.borrow_mut().remove(&self.type_id);
        });
    }
}

/// Trait implemented by registered values that need async cleanup.
#[async_trait]
pub trait Closeable: Send + Sync {
    /// Release resources held by this value.
    async fn close(&self) -> AppResult<()>;
}

/// Thread-safe runtime dependency injection container.
///
/// Each dependency is keyed by its concrete Rust type (`TypeId`). Three
/// registration modes are supported:
///
/// | Mode | Fn | Description |
/// |------|----|-|
/// | Eager | [`register`](Self::register) | Pre-built value, returned as-is on every resolve |
/// | Lazy factory | [`register_factory`](Self::register_factory) | Called fresh on every resolve |
/// | Singleton | [`register_singleton`](Self::register_singleton) | Called once; result cached |
///
/// For values that implement [`Closeable`], use [`register_closeable`](Self::register_closeable)
/// or [`register_singleton_closeable`](Self::register_singleton_closeable) so [`close`](Self::close) can clean them up.
pub struct Container {
    registrations: RwLock<HashMap<TypeId, Registration>>,
    closeables: RwLock<HashMap<TypeId, CloseableRegistration>>,
}

impl Default for Container {
    fn default() -> Self {
        Self::new()
    }
}

impl Container {
    /// Create an empty container.
    #[must_use]
    pub fn new() -> Self {
        Self {
            registrations: RwLock::new(HashMap::new()),
            closeables: RwLock::new(HashMap::new()),
        }
    }

    /// Register a pre-built value.
    pub fn register<T: Send + Sync + 'static>(&self, value: Arc<T>) {
        let type_id = TypeId::of::<T>();
        self.registrations
            .write()
            .insert(type_id, Registration::Eager(value as ArcAny));
        self.closeables.write().remove(&type_id);
    }

    /// Register a pre-built value that implements [`Closeable`].
    pub fn register_closeable<T: Closeable + Send + Sync + 'static>(&self, value: Arc<T>) {
        let type_id = TypeId::of::<T>();
        self.registrations
            .write()
            .insert(type_id, Registration::Eager(Arc::clone(&value) as ArcAny));
        self.closeables
            .write()
            .insert(type_id, CloseableRegistration::Eager(value as CloseableArc));
    }

    /// Register a factory called fresh on every resolve.
    pub fn register_factory<T, F>(&self, factory: F)
    where
        T: Send + Sync + 'static,
        F: Fn() -> AppResult<Arc<T>> + Send + Sync + 'static,
    {
        let type_id = TypeId::of::<T>();
        let factory: Factory = Arc::new(move || factory().map(|value| value as ArcAny));
        self.registrations
            .write()
            .insert(type_id, Registration::Lazy(factory));
        self.closeables.write().remove(&type_id);
    }

    /// Register a singleton factory — called once and cached.
    pub fn register_singleton<T, F>(&self, factory: F)
    where
        T: Send + Sync + 'static,
        F: Fn() -> AppResult<Arc<T>> + Send + Sync + 'static,
    {
        let type_id = TypeId::of::<T>();
        let registration = Arc::new(SingletonRegistration::new(Arc::new(move || {
            factory().map(|value| SingletonValue {
                any: value as ArcAny,
                closeable: None,
            })
        })));
        self.registrations
            .write()
            .insert(type_id, Registration::Singleton(registration));
        self.closeables.write().remove(&type_id);
    }

    /// Register a singleton factory for a type that implements [`Closeable`].
    pub fn register_singleton_closeable<T, F>(&self, factory: F)
    where
        T: Closeable + Send + Sync + 'static,
        F: Fn() -> AppResult<Arc<T>> + Send + Sync + 'static,
    {
        let type_id = TypeId::of::<T>();
        let registration = Arc::new(SingletonRegistration::new(Arc::new(move || {
            factory().map(|value| SingletonValue {
                any: Arc::clone(&value) as ArcAny,
                closeable: Some(value as CloseableArc),
            })
        })));
        self.registrations
            .write()
            .insert(type_id, Registration::Singleton(Arc::clone(&registration)));
        self.closeables
            .write()
            .insert(type_id, CloseableRegistration::Singleton(registration));
    }

    /// Resolve a registered type, returning `Err(NotFound)` if not registered.
    pub fn resolve<T: Send + Sync + 'static>(&self) -> AppResult<Arc<T>> {
        let _guard = ResolutionGuard::enter::<T>()?;
        let registration = self
            .registrations
            .read()
            .get(&TypeId::of::<T>())
            .cloned()
            .ok_or_else(|| AppError::not_found(std::any::type_name::<T>(), None))?;

        let value = match registration {
            Registration::Eager(value) => value,
            Registration::Lazy(factory) => factory()?,
            Registration::Singleton(registration) => registration.resolve_any()?,
        };

        value
            .downcast::<T>()
            .map_err(|_| AppError::new(ErrorCode::Internal, "type downcast failed in DI container"))
    }

    /// Returns `true` if `T` has been registered.
    #[must_use]
    pub fn is_registered<T: 'static>(&self) -> bool {
        self.registrations.read().contains_key(&TypeId::of::<T>())
    }

    /// Call [`Closeable::close`] on all registered closeable values once.
    pub async fn close(&self) -> AppResult<()> {
        let closeables = std::mem::take(&mut *self.closeables.write());
        for registration in closeables.into_values() {
            match registration {
                CloseableRegistration::Eager(closeable) => closeable.close().await?,
                CloseableRegistration::Singleton(registration) => {
                    if let Some(closeable) = registration.resolve_closeable()? {
                        closeable.close().await?;
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use super::{Closeable, Container};
    use rskit_errors::AppResult;

    struct Svc {
        val: usize,
    }

    #[test]
    fn register_and_resolve_eager() {
        let container = Container::new();
        container.register(Arc::new(Svc { val: 42 }));
        let service = container.resolve::<Svc>().expect("service should resolve");
        assert_eq!(service.val, 42);
    }

    #[test]
    fn resolve_unregistered_returns_not_found() {
        let container = Container::new();
        assert!(container.resolve::<Svc>().is_err());
    }

    #[test]
    fn register_factory_creates_new_arc_each_time() {
        let container = Container::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let captured = Arc::clone(&counter);
        container.register_factory(move || {
            let value = captured.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(Svc { val: value }))
        });

        let first = container
            .resolve::<Svc>()
            .expect("first resolve should work");
        let second = container
            .resolve::<Svc>()
            .expect("second resolve should work");

        assert_ne!(first.val, second.val);
        assert!(!Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn register_singleton_returns_same_instance() {
        let container = Container::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let captured = Arc::clone(&counter);
        container.register_singleton(move || {
            let value = captured.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(Svc { val: value }))
        });

        let first = container
            .resolve::<Svc>()
            .expect("first resolve should work");
        let second = container
            .resolve::<Svc>()
            .expect("second resolve should work");

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn circular_dependency_returns_error() {
        #[derive(Debug)]
        struct A;
        #[derive(Debug)]
        struct B;

        let container = Arc::new(Container::new());
        let a_container = Arc::clone(&container);
        container.register_factory::<A, _>(move || {
            let _ = a_container.resolve::<B>()?;
            Ok(Arc::new(A))
        });

        let b_container = Arc::clone(&container);
        container.register_factory::<B, _>(move || {
            let _ = b_container.resolve::<A>()?;
            Ok(Arc::new(B))
        });

        let error = container
            .resolve::<A>()
            .expect_err("circular dependency should fail");
        assert!(error.message().contains("circular dependency"));
    }

    struct MockCloseable {
        closed: Arc<AtomicBool>,
    }

    #[async_trait::async_trait]
    impl Closeable for MockCloseable {
        async fn close(&self) -> AppResult<()> {
            self.closed.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn close_calls_registered_closeables_once() {
        let container = Container::new();
        let closed = Arc::new(AtomicBool::new(false));
        container.register_closeable(Arc::new(MockCloseable {
            closed: Arc::clone(&closed),
        }));

        container.close().await.expect("close should succeed");
        container
            .close()
            .await
            .expect("second close should be a no-op");

        assert!(closed.load(Ordering::SeqCst));
    }
}
