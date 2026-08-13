//! Live-child registry keyed by process group id or child pid.

use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use parking_lot::Mutex;

#[derive(Debug, Clone, Copy)]
pub(super) struct RegisteredChild {
    pub(super) pid: u32,
    pub(super) process_group: bool,
}

#[derive(Debug, Default)]
pub(super) struct LiveChildRegistry {
    children: Mutex<HashMap<u32, RegisteredChild>>,
}

impl LiveChildRegistry {
    pub(super) fn register(self: &Arc<Self>, pid: u32, process_group: bool) -> RegistrationGuard {
        if pid != 0 {
            self.children
                .lock()
                .insert(pid, RegisteredChild { pid, process_group });
        }
        RegistrationGuard {
            registry: Arc::clone(self),
            key: pid,
            armed: AtomicBool::new(pid != 0),
        }
    }

    pub(super) fn unregister(&self, pid: u32) {
        if pid != 0 {
            self.children.lock().remove(&pid);
        }
    }

    pub(super) fn snapshot(&self) -> Vec<RegisteredChild> {
        self.children.lock().values().copied().collect()
    }

    pub(super) fn len(&self) -> usize {
        self.children.lock().len()
    }
}

/// RAII registration for a spawned child.
///
/// Dropping the guard unregisters the child if its normal reap path did not already do so. Explicit unregister and drop are idempotent, so racing cleanup paths collapse to one registry removal.
#[derive(Debug)]
pub struct RegistrationGuard {
    registry: Arc<LiveChildRegistry>,
    key: u32,
    armed: AtomicBool,
}

impl RegistrationGuard {
    pub(crate) fn unregister(&self) {
        if self.armed.swap(false, Ordering::AcqRel) {
            self.registry.unregister(self.key);
        }
    }
}

impl Drop for RegistrationGuard {
    fn drop(&mut self) {
        self.unregister();
    }
}
