//! Child-side controlling-terminal setup, run after fork and before exec.

use std::io;

use tokio::process::Command as TokioCommand;

/// Install the `pre_exec` hook that makes the slave PTY the child's controlling
/// terminal.
///
/// By the time the hook runs, the spawn machinery has already dup'd the slave
/// onto the child's fds 0/1/2. The hook then:
/// 1. `setsid()` — start a new session so the child is a session leader with no
///    controlling terminal (a prerequisite for acquiring one), which also makes
///    it a process-group leader whose group id equals its pid.
/// 2. `ioctl(0, TIOCSCTTY)` — acquire the slave (now fd 0) as the controlling
///    terminal, so the child and its descendants see a real interactive tty.
///
/// Because `setsid` establishes the process group, this hook intentionally
/// replaces the plain `setpgid` isolation used for pipe-backed modes; the
/// resulting group id still equals the child pid. This preserves the same group
/// layout the pipe-backed path relies on, so when [`SignalPolicy`] is configured
/// to target the process group, group-targeted termination behaves identically
/// to the non-PTY modes; it does not change or override `SignalPolicy`.
///
/// [`SignalPolicy`]: crate::SignalPolicy
pub(crate) fn install_controlling_tty(cmd: &mut TokioCommand) {
    // SAFETY: the closure runs in the forked child before `exec`. It calls only
    // async-signal-safe syscalls (`setsid`, `ioctl`) and returns an `io::Error`
    // on failure, which aborts the spawn — the supported `pre_exec` contract.
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(io::Error::last_os_error());
            }
            // The controlling-tty ioctl takes an int arg on every supported
            // platform; `0` means "do not steal from another session".
            if libc::ioctl(libc::STDIN_FILENO, libc::TIOCSCTTY as _, 0) < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
}
