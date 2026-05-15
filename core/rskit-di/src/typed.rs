//! Typed convenience helpers for [`crate::Container`].

use std::sync::Arc;

use rskit_errors::AppResult;

use crate::Container;

/// Typed dependency resolver contract for constructors that need `T`.
pub trait Resolve<T>
where
    T: Send + Sync + 'static,
{
    /// Resolve `T` from the dependency graph.
    fn resolve(&self) -> AppResult<Arc<T>>;
}

/// Typed dependency resolver contract that panics when `T` is unavailable.
///
/// This is intended for tests and startup-only wiring where failure should abort
/// immediately. Runtime paths should prefer [`Resolve`].
pub trait MustResolve<T>
where
    T: Send + Sync + 'static,
{
    /// Resolve `T` or panic with a descriptive message.
    ///
    /// # Panics
    ///
    /// Panics when `T` is not registered or its factory returns an error.
    fn must_resolve(&self) -> Arc<T>;
}

impl<T> Resolve<T> for Container
where
    T: Send + Sync + 'static,
{
    fn resolve(&self) -> AppResult<Arc<T>> {
        Container::resolve::<T>(self)
    }
}

impl<T> MustResolve<T> for Container
where
    T: Send + Sync + 'static,
{
    fn must_resolve(&self) -> Arc<T> {
        must_resolve::<T>(self)
    }
}

/// Register a lazily constructed singleton for `T`.
pub fn provide<T, F>(container: &Container, factory: F)
where
    T: Send + Sync + 'static,
    F: Fn() -> AppResult<Arc<T>> + Send + Sync + 'static,
{
    container.register_singleton(factory);
}

/// Register a pre-built singleton instance for `T`.
pub fn provide_singleton<T>(container: &Container, value: Arc<T>)
where
    T: Send + Sync + 'static,
{
    container.register(value);
}

/// Register a transient factory for `T`.
pub fn provide_transient<T, F>(container: &Container, factory: F)
where
    T: Send + Sync + 'static,
    F: Fn() -> AppResult<Arc<T>> + Send + Sync + 'static,
{
    container.register_factory(factory);
}

/// Resolve `T` from the container.
pub fn resolve<T>(container: &Container) -> AppResult<Arc<T>>
where
    T: Send + Sync + 'static,
{
    container.resolve::<T>()
}

/// Resolve `T` or panic.
pub fn must_resolve<T>(container: &Container) -> Arc<T>
where
    T: Send + Sync + 'static,
{
    resolve::<T>(container)
        .unwrap_or_else(|error| panic!("failed to resolve {}: {error}", std::any::type_name::<T>()))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::{
        MustResolve, Resolve, must_resolve, provide, provide_singleton, provide_transient, resolve,
    };
    use crate::Container;

    #[derive(Debug)]
    struct Service {
        value: usize,
    }

    #[test]
    fn provide_registers_lazy_singleton() {
        let container = Container::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let captured = Arc::clone(&counter);
        provide(&container, move || {
            let value = captured.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(Service { value }))
        });

        let first = resolve::<Service>(&container).expect("first resolve should succeed");
        let second = resolve::<Service>(&container).expect("second resolve should succeed");

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn provide_singleton_registers_existing_value() {
        let container = Container::new();
        let service = Arc::new(Service { value: 42 });
        provide_singleton(&container, Arc::clone(&service));

        let resolved = must_resolve::<Service>(&container);
        assert!(Arc::ptr_eq(&service, &resolved));
    }

    #[test]
    fn provide_transient_registers_factory() {
        let container = Container::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let captured = Arc::clone(&counter);
        provide_transient(&container, move || {
            let value = captured.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(Service { value }))
        });

        let first = resolve::<Service>(&container).expect("first resolve should succeed");
        let second = resolve::<Service>(&container).expect("second resolve should succeed");

        assert_ne!(first.value, second.value);
        assert!(!Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn resolve_traits_delegate_to_container() {
        let container = Container::new();
        provide_singleton(&container, Arc::new(Service { value: 7 }));

        let via_trait = <Container as Resolve<Service>>::resolve(&container)
            .expect("trait resolve should succeed");
        assert_eq!(via_trait.value, 7);

        let via_must = <Container as MustResolve<Service>>::must_resolve(&container);
        assert_eq!(via_must.value, 7);
    }
}
