use std::collections::HashSet;
use std::time::Duration;
use std::{collections::HashMap, sync::Arc};

use parking_lot::RwLock;
use rskit_errors::{AppError, AppResult, ErrorCode};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::{Component, Health, RegistryConfig, State, StopResult};

#[derive(Clone)]
struct ComponentSnapshot {
    index: usize,
    name: String,
    component: Arc<dyn Component>,
    state: State,
}

struct RegisteredComponent {
    name: String,
    component: Arc<dyn Component>,
    state: State,
}

/// Ordered component registry.
///
/// Components start in registration order and stop in reverse registration
/// order, ensuring dependants shut down before their dependencies.
pub struct Registry {
    components: Arc<RwLock<Vec<RegisteredComponent>>>,
    config: RegistryConfig,
}

impl Default for Registry {
    fn default() -> Self {
        Self {
            components: Arc::new(RwLock::new(Vec::new())),
            config: RegistryConfig::default(),
        }
    }
}

impl Registry {
    /// Create an empty [`Registry`] with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a registry with custom [`RegistryConfig`].
    #[must_use]
    pub fn with_config(config: RegistryConfig) -> Self {
        Self {
            components: Arc::new(RwLock::new(Vec::new())),
            config,
        }
    }

    /// Register a component. Registration order determines startup order.
    pub fn register(&mut self, component: Arc<dyn Component>) {
        let entry = RegisteredComponent {
            name: component.name().to_string(),
            component,
            state: State::Created,
        };
        self.components.write().push(entry);
    }

    /// Start all startable components in registration order.
    pub async fn start_all(&self) -> AppResult<()> {
        let mut started_names = Vec::new();
        for snapshot in self.snapshots() {
            match snapshot.state {
                state if state.can_start() => {}
                State::Running => continue,
                state => {
                    return Err(AppError::new(
                        ErrorCode::Conflict,
                        format!(
                            "component '{}' cannot start from state {state}",
                            snapshot.name
                        ),
                    ));
                }
            }

            self.set_state(snapshot.index, State::Starting);
            tracing::debug!(component = snapshot.name, "starting component");
            match tokio::time::timeout(self.config.start_timeout, snapshot.component.start()).await
            {
                Ok(Ok(())) => {
                    self.set_state(snapshot.index, State::Running);
                    started_names.push(snapshot.name);
                    tracing::debug!(component = snapshot.component.name(), "component started");
                }
                Ok(Err(error)) => {
                    self.set_state(snapshot.index, State::Failed);
                    let rollback_results = self.rollback_started(&started_names).await;
                    let rollback_errors = rollback_results
                        .into_iter()
                        .filter_map(|result| {
                            result.error.map(|err| format!("{}: {err}", result.name))
                        })
                        .collect::<Vec<_>>();
                    return if rollback_errors.is_empty() {
                        Err(error)
                    } else {
                        Err(error
                            .context(format!("rollback failures: {}", rollback_errors.join("; "))))
                    };
                }
                Err(_) => {
                    self.set_state(snapshot.index, State::Failed);
                    match tokio::time::timeout(self.config.stop_timeout, snapshot.component.stop())
                        .await
                    {
                        Ok(Ok(())) => {
                            tracing::debug!(
                                component = snapshot.component.name(),
                                "component stopped after start timeout"
                            );
                        }
                        Ok(Err(error)) => {
                            tracing::error!(
                                component = snapshot.component.name(),
                                error = %error,
                                "component start timed out and stop cleanup failed"
                            );
                        }
                        Err(_) => {
                            tracing::error!(
                                component = snapshot.component.name(),
                                "component start timed out and stop cleanup also timed out"
                            );
                        }
                    }
                    let rollback_results = self.rollback_started(&started_names).await;
                    let rollback_errors = rollback_results
                        .into_iter()
                        .filter_map(|result| {
                            result.error.map(|err| format!("{}: {err}", result.name))
                        })
                        .collect::<Vec<_>>();
                    let error = AppError::new(
                        ErrorCode::Timeout,
                        format!("component '{}' start timed out", snapshot.name),
                    );
                    return if rollback_errors.is_empty() {
                        Err(error)
                    } else {
                        Err(error
                            .context(format!("rollback failures: {}", rollback_errors.join("; "))))
                    };
                }
            }
        }
        Ok(())
    }

    /// Start all components concurrently, limited by [`RegistryConfig::concurrency`].
    pub async fn start_all_concurrent(&self, cancel: CancellationToken) -> AppResult<()> {
        let candidates = self
            .snapshots()
            .into_iter()
            .filter(|snapshot| snapshot.state.can_start())
            .collect::<Vec<_>>();

        if candidates.is_empty() {
            return Ok(());
        }

        let concurrency = if self.config.concurrency == 0 {
            candidates.len().max(1)
        } else {
            self.config.concurrency
        };

        let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency.max(1)));
        let mut join_set = JoinSet::new();
        let mut task_indexes = HashMap::new();
        let candidate_names = candidates
            .iter()
            .map(|snapshot| snapshot.name.clone())
            .collect::<Vec<_>>();

        for snapshot in candidates {
            self.set_state(snapshot.index, State::Starting);
            let snapshot_index = snapshot.index;
            let components = Arc::clone(&self.components);
            let semaphore = Arc::clone(&semaphore);
            let cancel = cancel.clone();
            let start_timeout = self.config.start_timeout;
            let abort_handle = join_set.spawn(async move {
                let _permit = semaphore.acquire_owned().await.map_err(|_| {
                    AppError::new(ErrorCode::Cancelled, "component startup was cancelled")
                })?;
                start_component_snapshot(components, snapshot, cancel, start_timeout).await
            });
            task_indexes.insert(abort_handle.id(), snapshot_index);
        }

        let mut first_error = None;
        while let Some(join_result) = join_set.join_next_with_id().await {
            match join_result {
                Ok((_id, Ok(()))) => {}
                Ok((_id, Err(error))) => {
                    if first_error.is_none() {
                        cancel.cancel();
                        first_error = Some(error);
                    }
                }
                Err(error) => {
                    if let Some(index) = task_indexes.get(&error.id()) {
                        self.set_state(*index, State::Failed);
                    }
                    if first_error.is_none() {
                        cancel.cancel();
                        first_error = Some(AppError::internal(error));
                    }
                }
            }
        }

        if let Some(error) = first_error {
            let rollback_results = self.rollback_started(&candidate_names).await;
            let rollback_errors = rollback_results
                .into_iter()
                .filter_map(|result| result.error.map(|err| format!("{}: {err}", result.name)))
                .collect::<Vec<_>>();
            return if rollback_errors.is_empty() {
                Err(error)
            } else {
                Err(error.context(format!("rollback failures: {}", rollback_errors.join("; "))))
            };
        }

        Ok(())
    }

    /// Stop all running components in reverse registration order.
    pub async fn stop_all(&self) -> AppResult<()> {
        let results = self.stop_all_detailed().await;
        let failures = results
            .iter()
            .filter_map(|result| {
                result
                    .error
                    .as_ref()
                    .map(|error| format!("{}: {error}", result.name))
            })
            .collect::<Vec<_>>();

        if failures.is_empty() {
            Ok(())
        } else {
            Err(AppError::new(
                ErrorCode::Internal,
                format!("failed to stop components: {}", failures.join("; ")),
            ))
        }
    }

    /// Stop all running components in reverse registration order with per-component results.
    pub async fn stop_all_detailed(&self) -> Vec<StopResult> {
        let snapshots = self
            .snapshots()
            .into_iter()
            .rev()
            .filter(|snapshot| snapshot.state.should_stop())
            .collect::<Vec<_>>();

        let mut results = Vec::with_capacity(snapshots.len());
        for snapshot in snapshots {
            self.set_state(snapshot.index, State::Stopping);
            tracing::debug!(component = snapshot.name, "stopping component");
            match tokio::time::timeout(self.config.stop_timeout, snapshot.component.stop()).await {
                Ok(Ok(())) => {
                    self.set_state(snapshot.index, State::Stopped);
                    tracing::debug!(component = snapshot.component.name(), "component stopped");
                    results.push(StopResult {
                        name: snapshot.name,
                        error: None,
                    });
                }
                Ok(Err(error)) => {
                    self.set_state(snapshot.index, State::Failed);
                    tracing::warn!(component = snapshot.component.name(), error = %error, "error stopping component");
                    results.push(StopResult {
                        name: snapshot.name,
                        error: Some(error),
                    });
                }
                Err(_) => {
                    self.set_state(snapshot.index, State::Failed);
                    let error = AppError::new(
                        ErrorCode::Timeout,
                        format!("component '{}' stop timed out", snapshot.name),
                    );
                    tracing::warn!(component = snapshot.component.name(), error = %error, "error stopping component");
                    results.push(StopResult {
                        name: snapshot.name,
                        error: Some(error),
                    });
                }
            }
        }
        results
    }

    /// Collect health for all registered components without holding the registry lock
    /// while component health is evaluated.
    #[must_use]
    pub fn health_all(&self) -> Vec<Health> {
        let snapshots = self.snapshots();
        snapshots
            .into_iter()
            .map(|snapshot| snapshot.component.health())
            .collect()
    }

    /// Return the tracked state for the named component.
    #[must_use]
    pub fn state(&self, name: &str) -> Option<State> {
        self.components
            .read()
            .iter()
            .find(|entry| entry.name == name)
            .map(|entry| entry.state)
    }

    /// Return the number of registered components.
    #[must_use]
    pub fn len(&self) -> usize {
        self.components.read().len()
    }

    /// Return `true` if no components have been registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.components.read().is_empty()
    }

    fn snapshots(&self) -> Vec<ComponentSnapshot> {
        let components = self.components.read();
        components
            .iter()
            .enumerate()
            .map(|(index, entry)| ComponentSnapshot {
                index,
                name: entry.name.clone(),
                component: Arc::clone(&entry.component),
                state: entry.state,
            })
            .collect()
    }

    fn set_state(&self, index: usize, state: State) {
        if let Some(entry) = self.components.write().get_mut(index) {
            entry.state = state;
        }
    }

    async fn rollback_started(&self, names: &[String]) -> Vec<StopResult> {
        let names = names.iter().cloned().collect::<HashSet<_>>();
        let snapshots = self
            .snapshots()
            .into_iter()
            .rev()
            .filter(|snapshot| names.contains(&snapshot.name) && snapshot.state.should_stop())
            .collect::<Vec<_>>();

        let mut results = Vec::with_capacity(snapshots.len());
        for snapshot in snapshots {
            self.set_state(snapshot.index, State::Stopping);
            match tokio::time::timeout(self.config.stop_timeout, snapshot.component.stop()).await {
                Ok(Ok(())) => {
                    self.set_state(snapshot.index, State::Stopped);
                    results.push(StopResult {
                        name: snapshot.name,
                        error: None,
                    });
                }
                Ok(Err(error)) => {
                    self.set_state(snapshot.index, State::Failed);
                    results.push(StopResult {
                        name: snapshot.name,
                        error: Some(error),
                    });
                }
                Err(_) => {
                    self.set_state(snapshot.index, State::Failed);
                    results.push(StopResult {
                        name: snapshot.name.clone(),
                        error: Some(AppError::new(
                            ErrorCode::Timeout,
                            format!("component '{}' stop timed out", snapshot.name),
                        )),
                    });
                }
            }
        }
        results
    }
}

async fn start_component_snapshot(
    components: Arc<RwLock<Vec<RegisteredComponent>>>,
    snapshot: ComponentSnapshot,
    cancel: CancellationToken,
    start_timeout: Duration,
) -> AppResult<()> {
    if cancel.is_cancelled() {
        restore_state(&components, snapshot.index, snapshot.state);
        tracing::warn!(
            component = snapshot.name,
            "startup cancelled before dispatch"
        );
        return Ok(());
    }

    tracing::debug!(component = snapshot.name, "starting component (concurrent)");
    let start = tokio::time::timeout(start_timeout, snapshot.component.start());
    tokio::pin!(start);
    tokio::select! {
        () = cancel.cancelled() => {
            restore_state(&components, snapshot.index, snapshot.state);
            tracing::warn!(
                component = snapshot.name,
                "startup cancelled during dispatch"
            );
            Ok(())
        }
        result = &mut start => match result {
            Ok(Ok(())) => {
            update_state(&components, snapshot.index, State::Running);
            tracing::debug!(component = snapshot.name, "component started");
            Ok(())
        }
            Ok(Err(error)) => {
            update_state(&components, snapshot.index, State::Failed);
            Err(error)
        }
            Err(_) => {
            update_state(&components, snapshot.index, State::Failed);
            Err(AppError::new(
                ErrorCode::Timeout,
                format!("component '{}' start timed out", snapshot.name),
            ))
        }
        }
    }
}

fn update_state(components: &Arc<RwLock<Vec<RegisteredComponent>>>, index: usize, state: State) {
    if let Some(entry) = components.write().get_mut(index) {
        entry.state = state;
    }
}

fn restore_state(
    components: &Arc<RwLock<Vec<RegisteredComponent>>>,
    index: usize,
    original_state: State,
) {
    update_state(components, index, original_state);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;
    use std::time::Duration;

    use parking_lot::Mutex;
    use rskit_errors::AppError;
    use tokio_util::sync::CancellationToken;

    use super::{Registry, RegistryConfig, restore_state};
    use crate::{Component, Health, State};

    struct MockComponent {
        name: String,
        start_count: Arc<AtomicUsize>,
        stop_count: Arc<AtomicUsize>,
        fail_on_start: bool,
        delay: Option<Duration>,
    }

    impl MockComponent {
        fn new(name: impl Into<String>) -> Self {
            Self {
                name: name.into(),
                start_count: Arc::new(AtomicUsize::new(0)),
                stop_count: Arc::new(AtomicUsize::new(0)),
                fail_on_start: false,
                delay: None,
            }
        }

        fn with_fail_on_start(mut self) -> Self {
            self.fail_on_start = true;
            self
        }

        fn with_delay(mut self, delay: Duration) -> Self {
            self.delay = Some(delay);
            self
        }
    }

    #[async_trait::async_trait]
    impl Component for MockComponent {
        fn name(&self) -> &str {
            &self.name
        }

        async fn start(&self) -> rskit_errors::AppResult<()> {
            if let Some(delay) = self.delay {
                tokio::time::sleep(delay).await;
            }
            if self.fail_on_start {
                return Err(AppError::service_unavailable(self.name.clone()));
            }
            self.start_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn stop(&self) -> rskit_errors::AppResult<()> {
            self.stop_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn health(&self) -> Health {
            Health::healthy(&self.name)
        }
    }

    struct BlockingHealthComponent {
        name: String,
        gate: Arc<Mutex<()>>,
        health_count: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl Component for BlockingHealthComponent {
        fn name(&self) -> &str {
            &self.name
        }

        async fn start(&self) -> rskit_errors::AppResult<()> {
            Ok(())
        }

        async fn stop(&self) -> rskit_errors::AppResult<()> {
            Ok(())
        }

        fn health(&self) -> Health {
            let _guard = self.gate.lock();
            self.health_count.fetch_add(1, Ordering::SeqCst);
            Health::healthy(&self.name)
        }
    }

    #[tokio::test]
    async fn state_transitions_track_start_stop_and_restart() {
        let component = Arc::new(MockComponent::new("svc"));
        let mut registry = Registry::new();
        registry.register(component);

        assert_eq!(registry.state("svc"), Some(State::Created));

        registry.start_all().await.expect("start should succeed");
        assert_eq!(registry.state("svc"), Some(State::Running));

        registry.stop_all().await.expect("stop should succeed");
        assert_eq!(registry.state("svc"), Some(State::Stopped));

        registry.start_all().await.expect("restart should succeed");
        assert_eq!(registry.state("svc"), Some(State::Running));
    }

    #[tokio::test]
    async fn start_all_skips_running_components_and_rejects_in_progress_states() {
        let component = Arc::new(MockComponent::new("svc"));
        let start_count = Arc::clone(&component.start_count);
        let mut registry = Registry::new();
        registry.register(component);

        registry.start_all().await.expect("start should succeed");
        registry
            .start_all()
            .await
            .expect("running components should be skipped");
        assert_eq!(start_count.load(Ordering::SeqCst), 1);

        registry.set_state(0, State::Starting);
        let error = registry.start_all().await.unwrap_err();
        assert_eq!(error.code(), rskit_errors::ErrorCode::Conflict);
        assert!(error.message().contains("cannot start from state starting"));
    }

    #[test]
    fn registry_accessors_and_private_state_helpers_cover_empty_and_missing_indexes() {
        let mut registry = Registry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
        registry.set_state(42, State::Failed);

        let component = Arc::new(MockComponent::new("svc"));
        assert!(component.health().is_healthy());
        registry.register(component);
        assert!(!registry.is_empty());
        assert_eq!(registry.len(), 1);

        restore_state(&registry.components, 42, State::Created);
        restore_state(&registry.components, 0, State::Stopped);
        assert_eq!(registry.state("svc"), Some(State::Stopped));
    }

    #[tokio::test]
    async fn start_failure_rolls_back_started_components() {
        let started = Arc::new(MockComponent::new("started"));
        let failing = Arc::new(MockComponent::new("failing").with_fail_on_start());
        let never = Arc::new(MockComponent::new("never"));

        let started_stop_count = Arc::clone(&started.stop_count);
        let never_start_count = Arc::clone(&never.start_count);

        let mut registry = Registry::new();
        registry.register(started);
        registry.register(failing);
        registry.register(never);

        let result = registry.start_all().await;
        assert!(result.is_err());
        assert_eq!(registry.state("started"), Some(State::Stopped));
        assert_eq!(registry.state("failing"), Some(State::Failed));
        assert_eq!(registry.state("never"), Some(State::Created));
        assert_eq!(started_stop_count.load(Ordering::SeqCst), 1);
        assert_eq!(never_start_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn start_timeout_marks_component_failed() {
        let component = Arc::new(MockComponent::new("slow").with_delay(Duration::from_secs(60)));
        let mut registry = Registry::with_config(RegistryConfig {
            start_timeout: Duration::from_secs(1),
            ..RegistryConfig::default()
        });
        registry.register(component);

        let error = registry
            .start_all()
            .await
            .expect_err("slow component should time out");
        assert_eq!(error.code(), rskit_errors::ErrorCode::Timeout);
        assert_eq!(registry.state("slow"), Some(State::Failed));
    }

    #[tokio::test(start_paused = true)]
    async fn start_timeout_reports_rollback_stop_failures() {
        struct StopFailComponent(&'static str);

        #[async_trait::async_trait]
        impl Component for StopFailComponent {
            fn name(&self) -> &str {
                self.0
            }

            async fn start(&self) -> rskit_errors::AppResult<()> {
                Ok(())
            }

            async fn stop(&self) -> rskit_errors::AppResult<()> {
                Err(AppError::service_unavailable(self.0))
            }

            fn health(&self) -> Health {
                Health::healthy(self.0)
            }
        }

        let mut registry = Registry::with_config(RegistryConfig {
            start_timeout: Duration::from_secs(1),
            stop_timeout: Duration::from_secs(1),
            ..RegistryConfig::default()
        });
        registry.register(Arc::new(StopFailComponent("started")));
        registry.register(Arc::new(
            MockComponent::new("slow").with_delay(Duration::from_secs(60)),
        ));

        let error = registry.start_all().await.unwrap_err();

        assert_eq!(error.code(), rskit_errors::ErrorCode::Timeout);
        assert!(error.to_string().contains("rollback failures"));
        assert_eq!(registry.state("started"), Some(State::Failed));
        assert_eq!(registry.state("slow"), Some(State::Failed));
    }

    #[tokio::test(start_paused = true)]
    async fn start_failure_reports_rollback_stop_timeout() {
        struct SlowStopComponent;

        #[async_trait::async_trait]
        impl Component for SlowStopComponent {
            fn name(&self) -> &str {
                "slow-stop-started"
            }

            async fn start(&self) -> rskit_errors::AppResult<()> {
                Ok(())
            }

            async fn stop(&self) -> rskit_errors::AppResult<()> {
                tokio::time::sleep(Duration::from_secs(60)).await;
                Ok(())
            }

            fn health(&self) -> Health {
                Health::healthy(self.name())
            }
        }

        let mut registry = Registry::with_config(RegistryConfig {
            stop_timeout: Duration::from_secs(1),
            ..RegistryConfig::default()
        });
        registry.register(Arc::new(SlowStopComponent));
        registry.register(Arc::new(MockComponent::new("failing").with_fail_on_start()));

        let error = registry.start_all().await.unwrap_err();

        assert_eq!(error.code(), rskit_errors::ErrorCode::ServiceUnavailable);
        assert!(error.to_string().contains("rollback failures"));
        assert_eq!(registry.state("slow-stop-started"), Some(State::Failed));
        assert_eq!(registry.state("failing"), Some(State::Failed));
    }

    #[tokio::test(start_paused = true)]
    async fn concurrent_start_timeout_marks_component_failed_in_internal_suite() {
        let mut registry = Registry::with_config(RegistryConfig {
            start_timeout: Duration::from_secs(1),
            ..RegistryConfig::default()
        });
        registry.register(Arc::new(
            MockComponent::new("slow-concurrent").with_delay(Duration::from_secs(60)),
        ));

        let error = registry
            .start_all_concurrent(CancellationToken::new())
            .await
            .unwrap_err();

        assert_eq!(error.code(), rskit_errors::ErrorCode::Timeout);
        assert_eq!(registry.state("slow-concurrent"), Some(State::Failed));
    }

    #[test]
    fn health_all_supports_concurrent_snapshots() {
        let gate = Arc::new(Mutex::new(()));
        let health_count = Arc::new(AtomicUsize::new(0));

        let mut registry = Registry::with_config(RegistryConfig::default());
        registry.register(Arc::new(BlockingHealthComponent {
            name: "one".to_string(),
            gate: Arc::clone(&gate),
            health_count: Arc::clone(&health_count),
        }));
        registry.register(Arc::new(BlockingHealthComponent {
            name: "two".to_string(),
            gate,
            health_count: Arc::clone(&health_count),
        }));

        let registry = Arc::new(registry);
        let handles = (0..4)
            .map(|_| {
                let registry = Arc::clone(&registry);
                thread::spawn(move || registry.health_all())
            })
            .collect::<Vec<_>>();

        for handle in handles {
            let results = handle.join().expect("health thread should succeed");
            assert_eq!(results.len(), 2);
        }

        assert_eq!(health_count.load(Ordering::SeqCst), 8);
    }

    #[tokio::test]
    async fn blocking_health_component_start_stop_and_health_are_all_exercised() {
        let gate = Arc::new(Mutex::new(()));
        let health_count = Arc::new(AtomicUsize::new(0));
        let mut registry = Registry::new();
        registry.register(Arc::new(BlockingHealthComponent {
            name: "blocking".to_string(),
            gate,
            health_count: Arc::clone(&health_count),
        }));

        registry.start_all().await.expect("start should succeed");
        assert!(registry.health_all()[0].is_healthy());
        registry.stop_all().await.expect("stop should succeed");

        assert_eq!(health_count.load(Ordering::SeqCst), 1);
        assert_eq!(registry.state("blocking"), Some(State::Stopped));
    }

    #[tokio::test]
    async fn stop_all_detailed_collects_all_errors() {
        struct StopFailComponent(&'static str);

        #[async_trait::async_trait]
        impl Component for StopFailComponent {
            fn name(&self) -> &str {
                self.0
            }

            async fn start(&self) -> rskit_errors::AppResult<()> {
                Ok(())
            }

            async fn stop(&self) -> rskit_errors::AppResult<()> {
                Err(AppError::service_unavailable(self.0))
            }

            fn health(&self) -> Health {
                Health::healthy(self.0)
            }
        }

        let mut registry = Registry::new();
        registry.register(Arc::new(StopFailComponent("a")));
        registry.register(Arc::new(StopFailComponent("b")));
        registry.start_all().await.expect("start should succeed");
        assert!(registry.health_all().iter().all(Health::is_healthy));

        let results = registry.stop_all_detailed().await;
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|result| result.error.is_some()));
        assert_eq!(registry.state("a"), Some(State::Failed));
        assert_eq!(registry.state("b"), Some(State::Failed));
    }

    #[tokio::test(start_paused = true)]
    async fn stop_timeout_is_reported_per_component() {
        struct SlowStopComponent;

        #[async_trait::async_trait]
        impl Component for SlowStopComponent {
            fn name(&self) -> &str {
                "slow-stop"
            }

            async fn start(&self) -> rskit_errors::AppResult<()> {
                Ok(())
            }

            async fn stop(&self) -> rskit_errors::AppResult<()> {
                tokio::time::sleep(Duration::from_secs(60)).await;
                Ok(())
            }

            fn health(&self) -> Health {
                Health::healthy(self.name())
            }
        }

        let mut registry = Registry::with_config(RegistryConfig {
            stop_timeout: Duration::from_secs(1),
            ..RegistryConfig::default()
        });
        registry.register(Arc::new(SlowStopComponent));
        registry.start_all().await.expect("start should succeed");
        assert!(registry.health_all()[0].is_healthy());

        let results = registry.stop_all_detailed().await;
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].error.as_ref().map(|error| error.code()),
            Some(rskit_errors::ErrorCode::Timeout)
        );
        assert_eq!(registry.state("slow-stop"), Some(State::Failed));
    }

    #[tokio::test]
    async fn concurrent_start_all_rolls_back_running_components_after_failure() {
        let ok = Arc::new(MockComponent::new("ok").with_delay(Duration::from_millis(5)));
        let fail = Arc::new(MockComponent::new("fail").with_fail_on_start());

        let ok_stop_count = Arc::clone(&ok.stop_count);

        let mut registry = Registry::with_config(RegistryConfig {
            concurrency: 2,
            ..RegistryConfig::default()
        });
        registry.register(ok);
        registry.register(fail);

        let result = registry
            .start_all_concurrent(CancellationToken::new())
            .await;
        assert!(result.is_err());
        let ok_state = registry.state("ok");
        assert!(matches!(ok_state, Some(State::Created | State::Stopped)));
        assert_eq!(registry.state("fail"), Some(State::Failed));
        if ok_state == Some(State::Stopped) {
            assert_eq!(ok_stop_count.load(Ordering::SeqCst), 1);
        }
    }
}
