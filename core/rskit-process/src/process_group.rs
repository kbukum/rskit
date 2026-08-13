//! Process-group helpers for subprocess isolation and termination.

use std::process::Command as StdCommand;

use tokio::process::Command as TokioCommand;

use crate::signal::ProcessSignal;

/// Configure a command so the spawned child becomes the leader of a new process group.
pub fn isolate(command: &mut StdCommand) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;

        // SAFETY: `pre_exec` runs in the child process after fork and before exec.
        // The closure only calls the async-signal-safe `setpgid` libc function
        // and returns an `io::Error` on failure, which is the supported usage pattern.
        unsafe {
            command.pre_exec(|| {
                if libc::setpgid(0, 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    #[cfg(not(unix))]
    {
        let _ = command;
    }
}

/// Configure a Tokio command so the spawned child becomes the leader of a new process group.
pub fn isolate_async(command: &mut TokioCommand) {
    #[cfg(unix)]
    {
        command.kill_on_drop(true);
        // SAFETY: `pre_exec` runs in the child process after fork and before exec.
        // The closure only calls the async-signal-safe `setpgid` libc function and returns an `io::Error` on failure, which is the supported usage pattern.
        unsafe {
            command.pre_exec(|| {
                if libc::setpgid(0, 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    #[cfg(not(unix))]
    {
        let _ = command;
    }
}

/// Request graceful interruption for the process group led by the provided child PID.
#[must_use]
pub fn interrupt(pid: u32) -> bool {
    signal(pid, ProcessSignal::Interrupt)
}

/// Request graceful termination for the process group led by the provided child PID.
#[must_use]
pub fn terminate(pid: u32) -> bool {
    signal(pid, ProcessSignal::Terminate)
}

/// Forcefully terminate the process group led by the provided child PID.
#[must_use]
pub fn kill(pid: u32) -> bool {
    signal(pid, ProcessSignal::Kill)
}

pub(crate) fn terminate_target(pid: u32, process_group: bool) -> bool {
    signal_target(pid, ProcessSignal::Terminate, process_group)
}

pub(crate) fn kill_target(pid: u32, process_group: bool) -> bool {
    signal_target(pid, ProcessSignal::Kill, process_group)
}

/// Best-effort liveness probe for a still-signalable target pid.
///
/// Returns `true` while a process with this pid exists (or exists but is owned by
/// another user), and `false` once it is gone. This is a liveness check only — it
/// cannot distinguish an original child from an unrelated process that reused the pid.
pub(crate) fn target_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(unix)]
    {
        let Ok(pid) = i32::try_from(pid) else {
            return false;
        };
        // SAFETY: signal 0 performs an existence check without delivering a signal.
        // EPERM means the process exists but is owned by another user.
        unsafe {
            if libc::kill(pid, 0) == 0 {
                return true;
            }
            std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
        }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

fn signal(pid: u32, signal: ProcessSignal) -> bool {
    signal_target(pid, signal, true)
}

pub(crate) fn signal_target(pid: u32, signal: ProcessSignal, process_group: bool) -> bool {
    if pid == 0 {
        return false;
    }

    #[cfg(unix)]
    {
        // A live Unix PID always fits in `i32`; a value that does not cannot name a real process,
        // so there is nothing to signal.
        let Ok(pid) = i32::try_from(pid) else {
            return false;
        };
        let target = if process_group { -pid } else { pid };
        // SAFETY: `kill` targets either the child pid
        // or the negated process-group id created by [`isolate`].
        // ESRCH means the target has already exited.
        unsafe {
            let result = libc::kill(target, signal.as_raw());
            if result != 0 {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::ESRCH) {
                    return false;
                }
            }
            true
        }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        let _ = signal;
        let _ = process_group;
        false
    }
}

#[cfg(test)]
mod tests {
    use std::process::Stdio;
    use std::time::Duration;

    use super::*;

    #[cfg(unix)]
    #[test]
    fn public_helpers_signal_a_real_process_group_leader() {
        use std::os::unix::process::ExitStatusExt;

        // Spawn a child that becomes its own process-group leader via `isolate`,
        // so the public `terminate(pid)` helper (which targets the negated process-group id) reaches it.
        let mut command = StdCommand::new("/bin/sh");
        command
            .args(["-c", "sleep 30"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        isolate(&mut command);
        let mut child = command.spawn().expect("group-leader child spawns");
        let pid = child.id();

        assert!(terminate(pid), "terminating the process group succeeds");

        let status = child.wait().expect("child is reaped");
        assert_eq!(
            status.signal(),
            Some(ProcessSignal::Terminate.as_raw()),
            "child should be terminated by the group SIGTERM"
        );
    }

    #[test]
    fn process_group_helpers_reject_zero_pid() {
        assert!(!interrupt(0));
        assert!(!terminate(0));
        assert!(!kill(0));
        assert!(!terminate_target(0, false));
        assert!(!kill_target(0, false));
    }

    #[test]
    fn terminate_and_kill_targets_can_signal_child_processes() {
        let mut terminate_child = StdCommand::new("python3")
            .args(["-c", "import time; time.sleep(30)"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let terminate_pid = terminate_child.id();
        assert!(terminate_target(terminate_pid, false));
        let _ = terminate_child.wait().unwrap();

        let mut kill_child = StdCommand::new("python3")
            .args(["-c", "import time; time.sleep(30)"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let kill_pid = kill_child.id();
        assert!(kill_target(kill_pid, false));
        let _ = kill_child.wait().unwrap();

        std::thread::sleep(Duration::from_millis(10));
    }
}
