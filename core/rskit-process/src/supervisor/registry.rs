//! Live-child registry of owned, reuse-proof targets.
//!
//! Each spawned child is tracked by a generation-stamped entry holding an
//! [`OwnedTarget`] and an atomic claim state, rather than a bare pid. The
//! generation id makes every registration distinct even when the OS later
//! recycles a numeric pid, and the claim state lets a shutdown fan-out take
//! exclusive ownership of an entry before signalling so a concurrent waiter
//! cannot reap-and-recycle the pid underneath it.

use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
};

use parking_lot::Mutex;

use super::target::{OwnedChild, OwnedTarget};
use crate::command::LifecyclePolicy;

/// Available for a waiter or a shutdown fan-out to claim.
const STATE_LIVE: u8 = 0;
/// Claimed by an in-progress shutdown fan-out task.
const STATE_CLAIMED: u8 = 1;
/// Deliberately survived this shutdown pass (for example `kill_after_grace = false`);
/// excluded from further claims until the next shutdown resets it.
const STATE_SURVIVED: u8 = 2;
/// Reaped by the normal child owner while a shutdown claim was in flight.
const STATE_REAPED: u8 = 3;

/// A registered child: its owned target plus a claim state machine.
#[derive(Debug)]
struct Entry {
    target: Arc<OwnedTarget>,
    state: AtomicU8,
}

/// Registry of live children keyed by a monotonic generation id.
#[derive(Debug)]
pub(super) struct LiveChildRegistry {
    entries: Mutex<HashMap<u64, Arc<Entry>>>,
    next_id: AtomicU64,
    shutting_down: AtomicBool,
    /// Serializes whole shutdown passes so a second concurrent `shutdown()` never
    /// observes another pass's in-flight claimed entries as "drained" and returns
    /// before those children finish terminating.
    shutdown_gate: tokio::sync::Mutex<()>,
}

impl Default for LiveChildRegistry {
    fn default() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            shutting_down: AtomicBool::new(false),
            shutdown_gate: tokio::sync::Mutex::new(()),
        }
    }
}

impl LiveChildRegistry {
    /// Register a spawned child and return its RAII guard.
    ///
    /// A pid of `0` (an async child with no reported pid, or an explicit
    /// zero-pid registration) has no signalable identity, so no entry is created
    /// and the returned guard is inert.
    pub(super) fn register(
        self: &Arc<Self>,
        pid: u32,
        process_group: bool,
        policy: LifecyclePolicy,
    ) -> RegistrationGuard {
        if pid == 0 {
            return RegistrationGuard {
                registry: Arc::clone(self),
                id: 0,
                armed: AtomicBool::new(false),
            };
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let entry = Arc::new(Entry {
            target: Arc::new(OwnedTarget::new(pid, process_group, policy)),
            state: AtomicU8::new(STATE_LIVE),
        });
        self.entries.lock().insert(id, entry);
        RegistrationGuard {
            registry: Arc::clone(self),
            id,
            armed: AtomicBool::new(true),
        }
    }

    /// Atomically register a spawned child together with its owned handle.
    pub(super) fn register_owned(
        self: &Arc<Self>,
        pid: u32,
        process_group: bool,
        policy: LifecyclePolicy,
        child: OwnedChild,
    ) -> RegistrationGuard {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let target = Arc::new(OwnedTarget::new(pid, process_group, policy));
        target.attach_child(child);
        let entry = Arc::new(Entry {
            target,
            state: AtomicU8::new(STATE_LIVE),
        });
        self.entries.lock().insert(id, entry);
        RegistrationGuard {
            registry: Arc::clone(self),
            id,
            armed: AtomicBool::new(true),
        }
    }

    /// Remove the entry with the given generation id, if still present.
    pub(super) fn remove(&self, id: u64) {
        if id == 0 {
            return;
        }
        let mut entries = self.entries.lock();
        let Some(entry) = entries.get(&id) else {
            return;
        };
        if entry.state.load(Ordering::Acquire) == STATE_CLAIMED {
            entry.target.mark_reaped();
            entry.state.store(STATE_REAPED, Ordering::Release);
        } else {
            entries.remove(&id);
        }
    }

    /// Remove every entry currently tracking `pid`.
    ///
    /// Retained for the public `unregister_pid` convenience; ordinary cleanup
    /// goes through the generation-keyed [`RegistrationGuard`].
    pub(super) fn remove_pid(&self, pid: u32) {
        if pid == 0 {
            return;
        }
        let mut entries = self.entries.lock();
        entries.retain(|_, entry| {
            if entry.target.pid() != pid {
                return true;
            }
            if entry.state.load(Ordering::Acquire) == STATE_CLAIMED {
                entry.target.mark_reaped();
                entry.state.store(STATE_REAPED, Ordering::Release);
                true
            } else {
                false
            }
        });
    }

    /// Attach a relinquished child to the target with the given id so a later
    /// termination can reap it.
    pub(super) fn attach_child(&self, id: u64, child: OwnedChild) -> Option<OwnedChild> {
        let entries = self.entries.lock();
        let Some(entry) = entries.get(&id) else {
            return Some(child);
        };
        if !matches!(
            entry.state.load(Ordering::Acquire),
            STATE_LIVE | STATE_SURVIVED
        ) {
            return Some(child);
        }
        entry.target.attach_child(child);
        None
    }

    /// Force-kill and reap the child owned by one entry.
    pub(super) fn kill_owned(&self, id: u64) {
        let target = self
            .entries
            .lock()
            .get(&id)
            .map(|entry| Arc::clone(&entry.target));
        if let Some(target) = target {
            target.kill_blocking();
        }
    }

    /// Number of tracked children in any state.
    pub(super) fn len(&self) -> usize {
        self.entries.lock().len()
    }

    /// Acquire the exclusive shutdown gate, serializing whole fan-out passes.
    pub(super) async fn shutdown_gate(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.shutdown_gate.lock().await
    }

    /// Begin a shutdown pass: mark the registry shutting down and reset any
    /// survivors from a previous pass so they are eligible again.
    pub(super) fn start_shutdown(&self) {
        let entries = self.entries.lock();
        self.shutting_down.store(true, Ordering::Release);
        for entry in entries.values() {
            let _ = entry.state.compare_exchange(
                STATE_SURVIVED,
                STATE_LIVE,
                Ordering::AcqRel,
                Ordering::Relaxed,
            );
        }
    }

    /// Atomically claim every currently-live entry, returning their targets.
    ///
    /// Claiming under the registry lock excludes concurrent waiters and later
    /// registrations from the same entries, so a fan-out owns each target it
    /// signals until it either removes or releases it.
    pub(super) fn claim_live(&self, limit: usize) -> Vec<(u64, Arc<OwnedTarget>)> {
        let entries = self.entries.lock();
        let mut claimed = Vec::new();
        for (id, entry) in entries.iter() {
            if claimed.len() == limit {
                break;
            }
            if entry
                .state
                .compare_exchange(
                    STATE_LIVE,
                    STATE_CLAIMED,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                claimed.push((*id, Arc::clone(&entry.target)));
            }
        }
        claimed
    }

    /// Complete one shutdown claim and remove or retain the entry according to
    /// its outcome.
    pub(super) fn complete_claim(&self, id: u64, survived: bool) {
        let mut entries = self.entries.lock();
        let Some(entry) = entries.get(&id) else {
            return;
        };
        if entry.state.load(Ordering::Acquire) == STATE_REAPED || !survived {
            entries.remove(&id);
        } else {
            entry.state.store(STATE_SURVIVED, Ordering::Release);
        }
    }

    /// Restore unfinished claims after a shutdown future is cancelled.
    ///
    /// Entries reaped by their normal owner while claimed are removed; all
    /// others become live so the next shutdown can retry them.
    pub(super) fn release_claims(&self, claims: &[(u64, Arc<OwnedTarget>)]) {
        let mut entries = self.entries.lock();
        for (id, target) in claims {
            let Some(entry) = entries.get(id) else {
                continue;
            };
            if entry.state.load(Ordering::Acquire) == STATE_REAPED || target.confirmed_gone() {
                entries.remove(id);
            } else {
                entry.state.store(STATE_LIVE, Ordering::Release);
            }
        }
    }

    /// Finish the shutdown pass if no live entries remain.
    ///
    /// Late registrations that arrived while the fan-out was draining are live,
    /// so this returns `false` and the caller loops to claim them. Only once the
    /// registry holds no live entry — under the same lock that registration
    /// takes — is the shutting-down flag cleared and the pass allowed to end.
    pub(super) fn finish_shutdown_if_drained(&self) -> bool {
        let entries = self.entries.lock();
        let has_live = entries
            .values()
            .any(|entry| entry.state.load(Ordering::Acquire) == STATE_LIVE);
        if has_live {
            return false;
        }
        self.shutting_down.store(false, Ordering::Release);
        true
    }

    /// Snapshot every tracked target, for the supervisor's drop backstop.
    pub(super) fn drain_targets(&self) -> Vec<(u64, Arc<OwnedTarget>)> {
        self.entries
            .lock()
            .iter()
            .map(|(id, entry)| (*id, Arc::clone(&entry.target)))
            .collect()
    }

    /// Fetch the target for a generation id (test-only accessor).
    #[cfg(test)]
    pub(super) fn target_for_test(&self, id: u64) -> Option<Arc<OwnedTarget>> {
        self.entries
            .lock()
            .get(&id)
            .map(|entry| Arc::clone(&entry.target))
    }
}

/// RAII registration for a spawned child.
///
/// Dropping the guard unregisters the child if its normal reap path did not
/// already do so. `retain` instead keeps the entry registered
/// after the run path has handed the live child to its owned target, so a later
/// shutdown or supervisor drop still reaps a child that deliberately outlived
/// its grace period. Explicit unregister, retain, and drop are idempotent, so
/// racing cleanup paths collapse to a single registry decision.
#[derive(Debug)]
pub struct RegistrationGuard {
    registry: Arc<LiveChildRegistry>,
    id: u64,
    armed: AtomicBool,
}

impl RegistrationGuard {
    /// Remove the entry from the registry now.
    pub(crate) fn unregister(&self) {
        if self.armed.swap(false, Ordering::AcqRel) {
            self.registry.remove(self.id);
        }
    }

    /// Disarm the guard without removing the entry.
    ///
    /// The registry keeps ownership of the target (and any child relinquished to
    /// it) until a confirmed termination removes it.
    pub(crate) fn retain(&self) {
        self.armed.store(false, Ordering::Release);
    }

    /// Force-kill and reap the child owned by this registration.
    pub(crate) fn kill_owned_child(&self) {
        self.registry.kill_owned(self.id);
    }

    /// Hand a still-live child to the registered target and keep the entry.
    ///
    /// Used when a run path's child deliberately survived its grace period
    /// (`kill_after_grace = false`): the child moves into its owned target so a
    /// later shutdown or supervisor drop reaps it, and the guard is disarmed so
    /// its drop neither removes the entry nor double-reaps the child.
    pub(crate) fn relinquish_child(&self, child: OwnedChild) -> Option<OwnedChild> {
        self.armed.store(false, Ordering::Release);
        self.registry.attach_child(self.id, child)
    }

    /// The generation id of the registered entry (test-only accessor).
    #[cfg(test)]
    pub(super) fn entry_id_for_test(&self) -> u64 {
        self.id
    }
}

impl Drop for RegistrationGuard {
    fn drop(&mut self) {
        self.unregister();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn spawn() -> (std::process::Child, u32) {
        let child = std::process::Command::new("/bin/sh")
            .args(["-c", "sleep 30"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("child spawns");
        let pid = child.id();
        (child, pid)
    }

    /// Claiming takes exclusive ownership of every live entry, so a concurrent
    /// waiter that later removes the entry (and lets the OS recycle the pid)
    /// cannot pull the target out from under an in-flight escalation: the
    /// fan-out still holds the claimed [`OwnedTarget`] it captured.
    #[test]
    fn claim_owns_targets_independently_of_waiter_removal() {
        let registry = Arc::new(LiveChildRegistry::default());
        let (mut child, pid) = spawn();
        let guard = registry.register(pid, false, LifecyclePolicy::default());
        let id = guard.id;

        registry.start_shutdown();
        let claimed = registry.claim_live(usize::MAX);
        assert_eq!(claimed.len(), 1);
        let (claimed_id, target) = &claimed[0];
        assert_eq!(*claimed_id, id);

        // A concurrent waiter reaps the child and unregisters it after the claim.
        let _ = child.kill();
        let _ = child.wait();
        registry.remove(id);
        assert_eq!(
            registry.len(),
            1,
            "the claimed entry remains until the fan-out observes the reap"
        );

        // The fan-out still owns the claimed target and sees the exact original
        // process is gone, so no live bystander is ever signalled. Completing
        // the claim then removes the waiter-reaped entry.
        assert!(!target.is_alive());
        registry.complete_claim(id, false);
        assert_eq!(registry.len(), 0);
    }

    /// A child registered while a fan-out is draining is not lost: it enters the
    /// registry LIVE, so the drained check refuses to finish until it is claimed.
    #[test]
    fn late_registration_blocks_drain_until_claimed() {
        let registry = Arc::new(LiveChildRegistry::default());
        registry.start_shutdown();
        assert!(
            registry.finish_shutdown_if_drained(),
            "an empty registry drains immediately"
        );

        // Simulate a shutdown pass in progress that just drained, then a late
        // registration arrives before the pass could end.
        registry.start_shutdown();
        let (mut child, pid) = spawn();
        let guard = registry.register(pid, false, LifecyclePolicy::default());
        assert!(
            !registry.finish_shutdown_if_drained(),
            "a live late registration keeps the pass running"
        );

        let claimed = registry.claim_live(usize::MAX);
        assert_eq!(claimed.len(), 1, "the late child is claimed, not missed");
        registry.complete_claim(claimed[0].0, false);
        assert!(
            registry.finish_shutdown_if_drained(),
            "the pass ends only once the late child is drained"
        );

        drop(guard);
        let _ = child.kill();
        let _ = child.wait();
    }

    /// A survivor is excluded from the remainder of a pass but reset to live on
    /// the next shutdown, so a deliberately-surviving child is retried later.
    #[test]
    fn survivor_is_excluded_then_reset_on_next_shutdown() {
        let registry = Arc::new(LiveChildRegistry::default());
        let (mut child, pid) = spawn();
        let guard = registry.register(pid, false, LifecyclePolicy::default());
        let id = guard.id;

        registry.start_shutdown();
        let claimed = registry.claim_live(usize::MAX);
        assert_eq!(claimed.len(), 1);
        registry.complete_claim(id, true);

        assert!(
            registry.claim_live(usize::MAX).is_empty(),
            "a survivor is not re-claimed within the same pass"
        );
        assert!(registry.finish_shutdown_if_drained());

        registry.start_shutdown();
        assert_eq!(
            registry.claim_live(usize::MAX).len(),
            1,
            "the next shutdown resets the survivor to live"
        );
        registry.complete_claim(id, false);

        drop(guard);
        let _ = child.kill();
        let _ = child.wait();
    }

    /// A survivor handoff cannot attach behind an in-flight shutdown claim. The
    /// child is returned to its current owner for local cleanup instead of being
    /// silently dropped after the claimed entry is removed.
    #[test]
    fn attach_returns_child_when_shutdown_already_claimed_entry() {
        let registry = Arc::new(LiveChildRegistry::default());
        let (child, pid) = spawn();
        let guard = registry.register(pid, false, LifecyclePolicy::default());
        let id = guard.id;

        registry.start_shutdown();
        assert_eq!(registry.claim_live(1).len(), 1);

        let returned = registry
            .attach_child(id, OwnedChild::Std(child))
            .expect("claimed entry rejects a late child handoff");
        let OwnedChild::Std(mut child) = returned else {
            panic!("blocking child is returned unchanged");
        };
        let _ = child.kill();
        let _ = child.wait();
        registry.complete_claim(id, false);
        drop(guard);
    }
}
