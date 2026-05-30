//! Process-group helpers for subprocess isolation and termination.

use std::process::Command as StdCommand;

use crate::signal::ProcessSignal;

/// Configure a command so the spawned child becomes the leader of a new process group.
pub fn isolate(command: &mut StdCommand) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;

        // SAFETY: `pre_exec` runs in the child process after fork and before exec.
        // The closure only calls the async-signal-safe `setpgid` libc function and
        // returns an `io::Error` on failure, which is the supported usage pattern.
        unsafe {
            command.pre_exec(|| {
                if libc::setpgid(0, 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
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

fn signal(pid: u32, signal: ProcessSignal) -> bool {
    #[cfg(unix)]
    {
        // SAFETY: `kill` targets the negated process-group id created by
        // [`isolate`]. ESRCH means the process group has already exited.
        unsafe {
            let result = libc::kill(-(pid as i32), signal.as_raw());
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
        false
    }
}
