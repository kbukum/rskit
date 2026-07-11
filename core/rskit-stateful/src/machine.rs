use parking_lot::Mutex;
use rskit_errors::{AppError, AppResult, ErrorCode};
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use std::time::SystemTime;

/// Marker trait for states managed by [`StateMachine`].
pub trait State: Clone + Send + Sync + 'static {}

impl<T> State for T where T: Clone + Send + Sync + 'static {}

type Guard<S, C> = Arc<dyn Fn(&S, &C) -> AppResult<()> + Send + Sync>;
type Action<S, C> = Arc<dyn Fn(&S, &S, &C) -> AppResult<()> + Send + Sync>;

/// A named transition from one state to another.
pub struct Transition<S, C> {
    name: String,
    from: Option<S>,
    to: S,
    guard: Option<Guard<S, C>>,
    action: Option<Action<S, C>>,
}

impl<S, C> Transition<S, C>
where
    S: State,
{
    /// Create a transition that may be applied from any current state.
    #[must_use]
    pub fn new(name: impl Into<String>, to: S) -> Self {
        Self {
            name: name.into(),
            from: None,
            to,
            guard: None,
            action: None,
        }
    }

    /// Restrict this transition to a specific source state.
    #[must_use]
    pub fn from(mut self, state: S) -> Self {
        self.from = Some(state);
        self
    }

    /// Add a guard that must allow the transition.
    #[must_use]
    pub fn with_guard(
        mut self,
        guard: impl Fn(&S, &C) -> AppResult<()> + Send + Sync + 'static,
    ) -> Self {
        self.guard = Some(Arc::new(guard));
        self
    }

    /// Add an action that runs after guard validation and before the transition commits.
    ///
    /// Returning an error aborts the transition without changing state or recording audit.
    #[must_use]
    pub fn with_action(
        mut self,
        action: impl Fn(&S, &S, &C) -> AppResult<()> + Send + Sync + 'static,
    ) -> Self {
        self.action = Some(Arc::new(action));
        self
    }

    /// Transition name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Point-in-time state snapshot.
#[derive(Debug, Clone)]
pub struct StateSnapshot<S> {
    /// Current state.
    pub state: S,
    /// Monotonic state version.
    pub version: u64,
}

/// Audit record emitted for each successful transition.
#[derive(Debug, Clone)]
pub struct AuditEntry<S, C> {
    /// Transition name.
    pub transition: String,
    /// Previous state.
    pub from: S,
    /// New state.
    pub to: S,
    /// Caller-supplied transition context.
    pub context: C,
    /// Version after the transition.
    pub version: u64,
    /// Wall-clock time when the transition was applied.
    pub recorded_at: SystemTime,
}

/// Persistence hook for snapshots and audit entries.
pub trait StatePersistence<S, C>: Send + Sync
where
    S: State,
{
    /// Persist a successful state transition.
    fn persist(&self, snapshot: &StateSnapshot<S>, audit: &AuditEntry<S, C>) -> AppResult<()>;
}

struct MachineState<S, C> {
    current: S,
    version: u64,
    audit_log: Vec<AuditEntry<S, C>>,
}

/// Typed state machine with guarded transitions, audit, and persistence hooks.
pub struct StateMachine<S, C> {
    state: Mutex<MachineState<S, C>>,
    transitions: HashMap<String, Transition<S, C>>,
    persistence: Vec<Arc<dyn StatePersistence<S, C>>>,
}

impl<S, C> StateMachine<S, C>
where
    S: State + Eq + Hash,
    C: Clone + Send + Sync + 'static,
{
    /// Create a state machine with an initial state.
    #[must_use]
    pub fn new(initial: S) -> Self {
        Self {
            state: Mutex::new(MachineState {
                current: initial,
                version: 0,
                audit_log: Vec::new(),
            }),
            transitions: HashMap::new(),
            persistence: Vec::new(),
        }
    }

    /// Register a transition.
    #[must_use]
    pub fn with_transition(mut self, transition: Transition<S, C>) -> Self {
        self.transitions.insert(transition.name.clone(), transition);
        self
    }

    /// Register a persistence hook.
    #[must_use]
    pub fn with_persistence(mut self, persistence: Arc<dyn StatePersistence<S, C>>) -> Self {
        self.persistence.push(persistence);
        self
    }

    /// Return the current state.
    #[must_use]
    pub fn state(&self) -> S {
        self.state.lock().current.clone()
    }

    /// Return the current snapshot.
    #[must_use]
    pub fn snapshot(&self) -> StateSnapshot<S> {
        let state = self.state.lock();
        StateSnapshot {
            state: state.current.clone(),
            version: state.version,
        }
    }

    /// Return a copy of the audit log.
    #[must_use]
    pub fn audit_log(&self) -> Vec<AuditEntry<S, C>> {
        self.state.lock().audit_log.clone()
    }

    /// Apply a named transition.
    pub fn apply(&self, name: &str, context: C) -> AppResult<StateSnapshot<S>> {
        let transition = self.transitions.get(name).ok_or_else(|| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("state transition '{name}' is not registered"),
            )
        })?;

        let mut state = self.state.lock();
        let from = state.current.clone();
        if let Some(required) = &transition.from
            && required != &from
        {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!("transition '{name}' cannot be applied from current state"),
            ));
        }
        if let Some(guard) = &transition.guard {
            guard(&from, &context)?;
        }

        let to = transition.to.clone();
        let next_version = state.version + 1;
        let snapshot = StateSnapshot {
            state: to.clone(),
            version: next_version,
        };

        if let Some(action) = &transition.action {
            action(&from, &to, &context)?;
        }

        let audit = AuditEntry {
            transition: transition.name.clone(),
            from,
            to: to.clone(),
            context,
            version: snapshot.version,
            recorded_at: SystemTime::now(),
        };

        for persistence in &self.persistence {
            persistence.persist(&snapshot, &audit)?;
        }
        state.current = to;
        state.version = next_version;
        state.audit_log.push(audit);

        Ok(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    enum OrderState {
        Draft,
        Submitted,
    }

    struct CountingPersistence(AtomicUsize);
    struct FailingPersistence;

    impl StatePersistence<OrderState, bool> for CountingPersistence {
        fn persist(
            &self,
            _snapshot: &StateSnapshot<OrderState>,
            _audit: &AuditEntry<OrderState, bool>,
        ) -> AppResult<()> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    impl StatePersistence<OrderState, bool> for FailingPersistence {
        fn persist(
            &self,
            _snapshot: &StateSnapshot<OrderState>,
            _audit: &AuditEntry<OrderState, bool>,
        ) -> AppResult<()> {
            Err(AppError::new(ErrorCode::Internal, "persist failed"))
        }
    }

    #[test]
    fn guarded_transition_updates_state_and_audit() {
        let persistence = Arc::new(CountingPersistence(AtomicUsize::new(0)));
        let machine = StateMachine::new(OrderState::Draft)
            .with_transition(
                Transition::new("submit", OrderState::Submitted)
                    .from(OrderState::Draft)
                    .with_guard(|_, allowed| {
                        if *allowed {
                            Ok(())
                        } else {
                            Err(AppError::new(ErrorCode::InvalidInput, "not allowed"))
                        }
                    }),
            )
            .with_persistence(persistence.clone());

        let snapshot = machine.apply("submit", true).unwrap();
        assert_eq!(snapshot.state, OrderState::Submitted);
        assert_eq!(snapshot.version, 1);
        assert_eq!(machine.audit_log().len(), 1);
        assert_eq!(persistence.0.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn action_failure_does_not_change_state_or_audit() {
        let machine = StateMachine::new(OrderState::Draft).with_transition(
            Transition::new("submit", OrderState::Submitted)
                .from(OrderState::Draft)
                .with_action(|_, _, _| Err(AppError::new(ErrorCode::Internal, "action failed"))),
        );

        let error = machine.apply("submit", true).unwrap_err();

        assert_eq!(error.code(), ErrorCode::Internal);
        assert_eq!(machine.state(), OrderState::Draft);
        assert_eq!(machine.snapshot().version, 0);
        assert!(machine.audit_log().is_empty());
    }

    #[test]
    fn persistence_failure_does_not_change_state_or_audit() {
        let machine = StateMachine::new(OrderState::Draft)
            .with_transition(
                Transition::new("submit", OrderState::Submitted).from(OrderState::Draft),
            )
            .with_persistence(Arc::new(FailingPersistence));

        let error = machine.apply("submit", true).unwrap_err();

        assert_eq!(error.code(), ErrorCode::Internal);
        assert_eq!(machine.state(), OrderState::Draft);
        assert_eq!(machine.snapshot().version, 0);
        assert!(machine.audit_log().is_empty());
    }

    #[test]
    fn unregistered_and_wrong_source_transitions_are_rejected() {
        let transition =
            Transition::new("submit", OrderState::Submitted).from(OrderState::Submitted);
        assert_eq!(transition.name(), "submit");
        let machine =
            StateMachine::<OrderState, bool>::new(OrderState::Draft).with_transition(transition);

        assert_eq!(
            machine.apply("missing", true).unwrap_err().code(),
            ErrorCode::InvalidInput
        );
        assert_eq!(
            machine.apply("submit", true).unwrap_err().code(),
            ErrorCode::InvalidInput
        );
    }
}
