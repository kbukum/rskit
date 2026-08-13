#![allow(missing_docs)]

use std::sync::Arc;

use rskit_process::{LifecyclePolicy, ProcessSupervisor};

#[test]
fn registry_registers_on_spawn_unregisters_once_and_ignores_double_unregister() {
    let supervisor = ProcessSupervisor::new(LifecyclePolicy::default());
    let guard = supervisor.track_pid(42);
    assert_eq!(supervisor.registry_len(), 1);
    drop(guard);
    assert_eq!(supervisor.registry_len(), 0);
    supervisor.unregister_pid(42);
    assert_eq!(supervisor.registry_len(), 0);
}

#[test]
fn registry_concurrent_register_unregister_is_race_clean() {
    let supervisor = Arc::new(ProcessSupervisor::new(LifecyclePolicy::default()));
    let mut threads = Vec::new();
    for worker in 0..8 {
        let supervisor = Arc::clone(&supervisor);
        threads.push(std::thread::spawn(move || {
            for offset in 0..250 {
                let pid = worker * 1_000 + offset + 1;
                let guard = supervisor.track_pid(pid);
                if offset % 2 == 0 {
                    supervisor.unregister_pid(pid);
                }
                drop(guard);
            }
        }));
    }
    for thread in threads {
        thread.join().expect("registry worker should not panic");
    }
    assert_eq!(supervisor.registry_len(), 0);
}
