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

    /// Return a handle to the singleton's closeable only if it has already been constructed.
    /// Clones the `Arc` (the closeable stays in singleton state) and never triggers construction,
    /// so an unresolved singleton contributes nothing to [`Container::close`].
    fn closeable_if_resolved(&self) -> Option<CloseableArc> {
        let guard = self.value.lock();
        guard
            .as_ref()
            .and_then(|value| value.closeable.as_ref().map(Arc::clone))
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

/// One recorded closeable, tracked in registration order
/// so [`Container::close`] can release them in reverse (LIFO):
/// a dependency registered before the resources built on top of it is released after them.
struct CloseableEntry {
    type_name: &'static str,
    registration: CloseableRegistration,
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
/// Each dependency is keyed by its concrete Rust type (`TypeId`).
/// Three registration modes are supported:
///
/// | Mode | Fn | Description |
/// |------|----|-|
/// | Eager | [`register`](Self::register) | Pre-built value, returned as-is on every resolve |
/// | Lazy factory | [`register_factory`](Self::register_factory) | Called fresh on every resolve |
/// | Singleton | [`register_singleton`](Self::register_singleton) | Called once; result cached |
///
/// For values that implement [`Closeable`], use [`register_closeable`](Self::register_closeable)
/// or [`register_singleton_closeable`](Self::register_singleton_closeable)
/// so [`close`](Self::close) can clean them up.
pub struct Container {
    registrations: RwLock<HashMap<TypeId, Registration>>,
    closeables: Mutex<Vec<CloseableEntry>>,
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
            closeables: Mutex::new(Vec::new()),
        }
    }

    /// Register a pre-built value.
    pub fn register<T: Send + Sync + 'static>(&self, value: Arc<T>) {
        self.registrations
            .write()
            .insert(TypeId::of::<T>(), Registration::Eager(value as ArcAny));
    }

    /// Register a pre-built value that implements [`Closeable`].
    ///
    /// The container owns the value's cleanup:
    /// [`close`](Self::close) releases it in reverse registration order.
    /// Re-registering the same type records an additional closeable,
    /// so a replaced resource is still closed.
    pub fn register_closeable<T: Closeable + Send + Sync + 'static>(&self, value: Arc<T>) {
        self.registrations.write().insert(
            TypeId::of::<T>(),
            Registration::Eager(Arc::clone(&value) as ArcAny),
        );
        self.closeables.lock().push(CloseableEntry {
            type_name: std::any::type_name::<T>(),
            registration: CloseableRegistration::Eager(value as CloseableArc),
        });
    }

    /// Register a factory called fresh on every resolve.
    pub fn register_factory<T, F>(&self, factory: F)
    where
        T: Send + Sync + 'static,
        F: Fn() -> AppResult<Arc<T>> + Send + Sync + 'static,
    {
        let factory: Factory = Arc::new(move || factory().map(|value| value as ArcAny));
        self.registrations
            .write()
            .insert(TypeId::of::<T>(), Registration::Lazy(factory));
    }

    /// Register a singleton factory — called once and cached.
    pub fn register_singleton<T, F>(&self, factory: F)
    where
        T: Send + Sync + 'static,
        F: Fn() -> AppResult<Arc<T>> + Send + Sync + 'static,
    {
        let registration = Arc::new(SingletonRegistration::new(Arc::new(move || {
            factory().map(|value| SingletonValue {
                any: value as ArcAny,
                closeable: None,
            })
        })));
        self.registrations
            .write()
            .insert(TypeId::of::<T>(), Registration::Singleton(registration));
    }

    /// Register a singleton factory for a type that implements [`Closeable`].
    ///
    /// The closeable slot is recorded at registration time
    /// and its disposer is captured when the singleton is first resolved;
    /// [`close`](Self::close) releases it in reverse registration order (LIFO).
    /// An unresolved singleton constructs nothing and is not closed.
    pub fn register_singleton_closeable<T, F>(&self, factory: F)
    where
        T: Closeable + Send + Sync + 'static,
        F: Fn() -> AppResult<Arc<T>> + Send + Sync + 'static,
    {
        let registration = Arc::new(SingletonRegistration::new(Arc::new(move || {
            factory().map(|value| SingletonValue {
                any: Arc::clone(&value) as ArcAny,
                closeable: Some(value as CloseableArc),
            })
        })));
        self.registrations.write().insert(
            TypeId::of::<T>(),
            Registration::Singleton(Arc::clone(&registration)),
        );
        self.closeables.lock().push(CloseableEntry {
            type_name: std::any::type_name::<T>(),
            registration: CloseableRegistration::Singleton(registration),
        });
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

    /// Close every recorded closeable once, in reverse registration order (LIFO),
    /// so a dependency is released after the resources built on top of it.
    /// This drains the recorded closeables,
    /// so a second call closes only closeables registered since the previous call (a no-op when none were).
    /// All closeables are closed even if some fail; the returned error aggregates every failure.
    /// Unresolved singletons construct nothing and are skipped.
    pub async fn close(&self) -> AppResult<()> {
        let closeables = std::mem::take(&mut *self.closeables.lock());
        let mut errors: Vec<AppError> = Vec::new();
        for entry in closeables.into_iter().rev() {
            let closeable = match entry.registration {
                CloseableRegistration::Eager(closeable) => Some(closeable),
                CloseableRegistration::Singleton(registration) => {
                    registration.closeable_if_resolved()
                }
            };
            if let Some(closeable) = closeable
                && let Err(err) = closeable.close().await
            {
                errors.push(
                    err.context(format!("close {}", entry.type_name))
                        .with_detail("closeable", entry.type_name),
                );
            }
        }
        match aggregate_close_errors(errors) {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }
}

/// Combine disposal failures into a single error. The first failure is returned as-is —
/// preserving its code, retryability, HTTP status, cause, and structured details —
/// with a summary of the remaining failures attached. Returns `None` when nothing failed.
fn aggregate_close_errors(errors: Vec<AppError>) -> Option<AppError> {
    let mut iter = errors.into_iter();
    let first = iter.next()?;
    let rest: Vec<String> = iter.map(|err| err.message().to_string()).collect();
    if rest.is_empty() {
        return Some(first);
    }
    let summary = format!(
        "(and {} more error(s) while closing container: {})",
        rest.len(),
        rest.join("; ")
    );
    Some(
        first
            .with_detail("additional_close_errors", rest)
            .hint(summary),
    )
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
