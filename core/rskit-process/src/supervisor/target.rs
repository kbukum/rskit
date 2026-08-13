//! Owned, reuse-proof child identity behind the supervisor's registry.
//!
//! A supervised child is tracked by an [`OwnedTarget`] rather than a bare `u32`
//! pid. On Linux the target owns a [`pidfd`](https://man7.org/linux/man-pages/man2/pidfd_open.2.html)
//! so signals reach the exact process regardless of pid recycling: once the
//! original process exits, its pidfd reports `ESRCH` forever and can never be
//! aimed at an unrelated process that later reused the numeric pid. On other
//! Unix targets the identity falls back to the pid (and process-group id when
//! descendant termination is requested) with a documented best-effort
//! guarantee. On non-Unix targets there is no `kill(2)`-style primitive, so the
//! target reports honestly that it cannot signal — it never claims a
//! termination that did not happen.
//!
//! Direct-child signalling is fully reuse-proof through the pidfd. Descendant
//! (process-group) signalling still goes through the numeric process-group id
//! because Linux exposes no group file descriptor; it is gated on the pidfd
//! liveness check of the group leader so the group signal is only sent while the
//! leader — our own direct child — is still alive and therefore still owns the
//! group id. This is the accepted best-effort boundary for group cleanup.

use std::process::ExitStatus;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use crate::command::LifecyclePolicy;
#[cfg(unix)]
use crate::process_group::signal_target;
use crate::process_group::target_alive;
use crate::signal::ProcessSignal;

/// A child handle owned by the registry so it can be reaped after a confirmed
/// termination even when the original run path relinquished it.
#[derive(Debug)]
pub(crate) enum OwnedChild {
    /// A blocking [`std::process::Child`].
    Std(std::process::Child),
    /// An async [`tokio::process::Child`].
    Tokio(tokio::process::Child),
}

impl OwnedChild {
    fn start_kill(&mut self) {
        match self {
            Self::Std(child) => {
                let _ = child.kill();
            }
            Self::Tokio(child) => {
                let _ = child.start_kill();
            }
        }
    }

    fn try_reap(&mut self) -> Option<ExitStatus> {
        match self {
            Self::Std(child) => child.try_wait().ok().flatten(),
            Self::Tokio(child) => child.try_wait().ok().flatten(),
        }
    }

    async fn reap(&mut self) {
        match self {
            Self::Std(child) => {
                let _ = child.wait();
            }
            Self::Tokio(child) => {
                let _ = child.wait().await;
            }
        }
    }
}

/// The outcome of terminating a target: whether it may be unregistered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TargetOutcome {
    /// The target is confirmed gone (or was never a real process); unregister it.
    Terminated,
    /// The target deliberately survived (`kill_after_grace = false`) or could not
    /// be signalled on this platform; keep it registered for a later attempt.
    Survived,
}

impl TargetOutcome {
    pub(super) fn is_terminated(self) -> bool {
        matches!(self, Self::Terminated)
    }
}

/// A Linux pidfd owning a stable, reuse-proof reference to one process.
#[cfg(target_os = "linux")]
#[derive(Debug)]
struct PidFd(std::os::fd::OwnedFd);

#[cfg(target_os = "linux")]
impl PidFd {
    /// Open a pidfd for `pid`, or `None` when the kernel lacks pidfd support or
    /// the process is already gone.
    fn open(pid: u32) -> Option<Self> {
        use std::os::fd::FromRawFd;

        let Ok(pid) = i32::try_from(pid) else {
            return None;
        };
        // SAFETY: `pidfd_open` takes a pid and flags and returns a new owned file
        // descriptor or -1. We pass a valid pid and no flags, and wrap the returned
        // descriptor in `OwnedFd` so it is closed exactly once.
        let raw = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
        if raw < 0 {
            return None;
        }
        let Ok(raw) = i32::try_from(raw) else {
            // A descriptor that does not fit in `c_int` cannot be used; close it.
            // SAFETY: `raw` is a descriptor the kernel just returned to this process.
            unsafe {
                libc::close(raw as libc::c_int);
            }
            return None;
        };
        // SAFETY: `raw` is a fresh, valid file descriptor with no other owner.
        Some(Self(unsafe { std::os::fd::OwnedFd::from_raw_fd(raw) }))
    }

    /// Send `sig` to the exact process this pidfd refers to.
    ///
    /// Returns `true` when the signal was delivered or the process has already
    /// exited (`ESRCH`); the pidfd guarantees this never targets a reused pid.
    fn send(&self, sig: libc::c_int) -> bool {
        use std::os::fd::AsRawFd;

        // SAFETY: `pidfd_send_signal` takes the pidfd, a signal number, a null
        // `siginfo_t` (kernel synthesises it), and no flags. The fd is owned and
        // valid for the call.
        let result = unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                self.0.as_raw_fd(),
                sig,
                std::ptr::null::<libc::siginfo_t>(),
                0,
            )
        };
        if result == 0 {
            return true;
        }
        std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
    }

    /// Return `true` while the referenced process still exists.
    ///
    /// Uses signal `0`, which performs delivery checks without sending a signal.
    /// A terminated-but-unreaped process (zombie) reports `ESRCH`, so this is a
    /// truthful "has the exact original process exited" probe.
    fn is_alive(&self) -> bool {
        use std::os::fd::AsRawFd;

        // SAFETY: identical contract to `send`; signal 0 is the existence check.
        let result = unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                self.0.as_raw_fd(),
                0,
                std::ptr::null::<libc::siginfo_t>(),
                0,
            )
        };
        result == 0
    }
}

/// Reuse-proof identity and reaping authority for one supervised child.
#[derive(Debug)]
pub(super) struct OwnedTarget {
    pid: u32,
    process_group: bool,
    #[cfg(target_os = "linux")]
    pidfd: Option<PidFd>,
    child: Mutex<Option<OwnedChild>>,
}

impl OwnedTarget {
    /// Build a target for `pid`, capturing a pidfd on Linux where available.
    pub(super) fn new(pid: u32, process_group: bool) -> Self {
        Self {
            pid,
            process_group,
            #[cfg(target_os = "linux")]
            pidfd: (pid != 0).then(|| PidFd::open(pid)).flatten(),
            child: Mutex::new(None),
        }
    }

    /// The numeric pid this target was registered under.
    pub(super) fn pid(&self) -> u32 {
        self.pid
    }

    /// Hand the live child to the target so a later termination can reap it.
    ///
    /// Used when a run path relinquishes a child that deliberately survived its
    /// grace period (`kill_after_grace = false`): ownership moves here so a
    /// subsequent shutdown or supervisor drop still reaps it instead of leaking.
    pub(super) fn attach_child(&self, child: OwnedChild) {
        *self.child.lock() = Some(child);
    }

    /// Return `true` while the exact original process is still alive.
    pub(super) fn is_alive(&self) -> bool {
        if self.pid == 0 {
            return false;
        }
        #[cfg(target_os = "linux")]
        if let Some(pidfd) = &self.pidfd {
            return pidfd.is_alive();
        }
        target_alive(self.pid)
    }

    /// Send `signal` to the target's leader (reuse-proof on Linux) and, when
    /// descendant termination is requested, to its process group.
    ///
    /// Returns `true` when the leader signal was delivered or the leader has
    /// already exited. The group signal is best-effort and gated on the leader
    /// still being alive so a reused process-group id is never targeted.
    fn signal(&self, signal: ProcessSignal) -> bool {
        if self.pid == 0 {
            return false;
        }
        #[cfg(target_os = "linux")]
        if let Some(pidfd) = &self.pidfd {
            let leader = pidfd.send(signal.as_raw());
            if self.process_group && pidfd.is_alive() {
                // The leader is our own direct child and still alive, so the
                // process-group id it owns cannot have been recycled: reaching
                // descendants through it is safe.
                let _ = signal_target(self.pid, signal, true);
            }
            return leader;
        }
        // Non-Linux Unix (and Linux without pidfd support): best-effort by pid /
        // process-group id, gated on a liveness probe for the group case.
        #[cfg(unix)]
        {
            if self.process_group {
                if target_alive(self.pid) {
                    return signal_target(self.pid, signal, true);
                }
                // The leader is gone; signal the pid directly rather than a
                // possibly-recycled group id.
                return signal_target(self.pid, signal, false);
            }
            signal_target(self.pid, signal, self.process_group)
        }
        #[cfg(not(unix))]
        {
            let _ = signal;
            false
        }
    }

    /// Terminate the target, waiting the policy grace period, and report whether
    /// it may be unregistered.
    ///
    /// Sends graceful termination, waits the grace period, and confirms exit
    /// through the reuse-proof identity. A target that is still alive with
    /// escalation disabled survives untouched and stays registered. With
    /// escalation enabled a still-alive target is force-killed; on platforms
    /// that cannot signal at all the target is reported [`Survived`] so it is not
    /// dropped from the registry as though it were reaped.
    ///
    /// [`Survived`]: TargetOutcome::Survived
    pub(super) async fn terminate(&self, policy: LifecyclePolicy) -> TargetOutcome {
        if self.pid == 0 {
            return TargetOutcome::Terminated;
        }
        if !cfg!(unix) {
            // No signalling primitive exists; never claim a termination.
            return TargetOutcome::Survived;
        }
        self.signal(ProcessSignal::Terminate);
        tokio::time::sleep(policy.grace_period).await;
        if !self.is_alive() {
            self.reap().await;
            return TargetOutcome::Terminated;
        }
        if !policy.kill_after_grace {
            return TargetOutcome::Survived;
        }
        self.signal(ProcessSignal::Kill);
        self.reap().await;
        TargetOutcome::Terminated
    }

    /// Reap the owned child, if any, so no zombie is left behind.
    ///
    /// When the target owns a relinquished child this awaits its exit. When the
    /// child is owned elsewhere (the normal run path reaps it itself) this is a
    /// no-op; the registry entry is removed only after that owner confirms exit.
    async fn reap(&self) {
        let child = self.child.lock().take();
        if let Some(mut child) = child {
            child.reap().await;
        }
    }

    /// Force-kill and reap synchronously, for the supervisor's drop backstop.
    ///
    /// Signals the target, then reaps an owned child within a bounded budget so a
    /// synchronous [`Drop`] never blocks indefinitely on a process that ignores
    /// signalling.
    pub(super) fn kill_blocking(&self) {
        if self.pid == 0 {
            return;
        }
        self.signal(ProcessSignal::Kill);
        let child = self.child.lock().take();
        if let Some(mut child) = child {
            child.start_kill();
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if child.try_reap().is_some() {
                    return;
                }
                if Instant::now() >= deadline {
                    return;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    fn spawn_std(args: &[&str]) -> std::process::Child {
        Command::new("/bin/sh")
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("child spawns")
    }

    /// The owned target reports the *exact original* process as gone once it is
    /// reaped, so any later signalling can only ever confirm the original is
    /// gone — never act on a process that reused the numeric pid.
    #[test]
    fn target_reports_not_alive_after_reap() {
        let child = spawn_std(&["-c", "sleep 30"]);
        let pid = child.id();
        let target = OwnedTarget::new(pid, false);
        target.attach_child(OwnedChild::Std(child));
        assert!(target.is_alive(), "freshly spawned child is alive");

        target.kill_blocking();
        assert!(
            !target.is_alive(),
            "the exact original process is gone after a confirmed reap"
        );
        assert_eq!(target.pid(), pid, "pid is retained for diagnostics only");
    }

    /// `terminate` with escalation enabled reaps a cooperative child and reports
    /// it terminated so the registry can drop the entry.
    #[tokio::test]
    async fn terminate_reaps_cooperative_child() {
        let child = tokio::process::Command::new("/bin/sh")
            .args(["-c", "sleep 30"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("child spawns");
        let pid = child.id().expect("live pid");
        let target = OwnedTarget::new(pid, false);
        target.attach_child(OwnedChild::Tokio(child));

        let policy = LifecyclePolicy::default().with_grace_period(Duration::from_millis(50));
        let outcome = target.terminate(policy).await;

        assert_eq!(outcome, TargetOutcome::Terminated);
        assert!(!target.is_alive(), "reaped child is gone");
    }

    /// A child that ignores `SIGTERM` under `kill_after_grace = false` is never
    /// force-killed and is reported as [`TargetOutcome::Survived`] so the
    /// registry keeps owning it instead of dropping it as though it exited.
    #[tokio::test]
    async fn stubborn_child_survives_without_force_kill() {
        let child = tokio::process::Command::new("/bin/sh")
            .args(["-c", "trap '' TERM; sleep 30"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("child spawns");
        let pid = child.id().expect("live pid");
        let target = OwnedTarget::new(pid, false);
        target.attach_child(OwnedChild::Tokio(child));

        let policy = LifecyclePolicy {
            kill_after_grace: false,
            ..LifecyclePolicy::default()
        }
        .with_grace_period(Duration::from_millis(50));
        let outcome = target.terminate(policy).await;

        assert_eq!(
            outcome,
            TargetOutcome::Survived,
            "no force-kill, so ownership is retained"
        );
        assert!(target.is_alive(), "the child is still running");

        // The target still owns the child, so the drop backstop reaps it.
        target.kill_blocking();
        assert!(!target.is_alive(), "backstop reaps the retained child");
    }

    /// On Linux the pidfd is a stable, reuse-proof identity: after the original
    /// process exits, the pidfd reports it gone and a signal through it resolves
    /// to `ESRCH`, so it can never be aimed at a process that reused the pid.
    #[cfg(target_os = "linux")]
    #[test]
    fn pidfd_identity_survives_process_exit() {
        let mut child = spawn_std(&["-c", "exit 0"]);
        let pid = child.id();
        let pidfd = PidFd::open(pid).expect("pidfd opens for a live pid");
        let _ = child.wait();

        assert!(!pidfd.is_alive(), "the exact original process has exited");
        assert!(
            pidfd.send(libc::SIGKILL),
            "signalling an exited pidfd resolves to ESRCH, hitting nothing"
        );
    }
}
