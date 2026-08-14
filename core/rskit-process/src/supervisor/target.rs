//! Owned, reuse-proof child identity behind the supervisor's registry.
//!
//! A supervised child is tracked by an [`OwnedTarget`] rather than a bare `u32`
//! pid. On Linux the target owns a [`pidfd`](https://man7.org/linux/man-pages/man2/pidfd_open.2.html)
//! so signals reach the exact process regardless of pid recycling: once the
//! original process exits, its pidfd reports `ESRCH` forever and can never be
//! aimed at an unrelated process that later reused the numeric pid. On other
//! Unix targets the identity falls back to the pid (and process-group id when
//! descendant termination is requested) with a documented best-effort
//! guarantee. On non-Unix targets there is no `kill(2)`-style primitive: the
//! target can still force-kill and reap a child handle it *owns* (through
//! [`std::process::Child`]/[`tokio::process::Child`], which work on every
//! platform), but for a child owned elsewhere it cannot signal at all and
//! reports [`TargetOutcome::Unsupported`] so shutdown surfaces the limitation
//! rather than claiming a termination that did not happen.
//!
//! Direct-child signalling is fully reuse-proof through the pidfd. Descendant
//! (process-group) signalling goes through the numeric process-group id because
//! Linux exposes no group file descriptor; it is gated on a `kill(-pgid, 0)`
//! probe that the group is still non-empty. POSIX keeps a process-group id
//! reserved while any member survives, so probing a non-empty group names the
//! original group even after its leader has exited — which is what lets
//! escalation reach a descendant that outlived the leader. This makes descendant
//! signalling best-effort rather than reuse-proof: the probe and the follow-up
//! `-pgid` signal are not atomic, so if the group empties in between the id can
//! be recycled and the signal reach an unrelated group. Closing that residual
//! window portably is not possible without a group-level ownership handle (a
//! Linux cgroup or a job object). Liveness of the *subtree* is therefore tracked
//! through the group probe, distinct from liveness of the leader (its
//! pidfd/pid), and a group target is only considered gone once its whole group
//! is empty.

use std::process::ExitStatus;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use crate::command::LifecyclePolicy;
#[cfg(unix)]
use crate::process_group::signal_target;
use crate::process_group::{group_alive, target_alive};
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
    /// The target deliberately survived (`kill_after_grace = false`); keep it
    /// registered so a later shutdown or drop can act.
    Survived,
    /// The platform has no way to signal the target and no owned child handle to
    /// kill, so termination could not even be attempted. The entry is kept
    /// registered and the caller reports the failure honestly rather than
    /// claiming a success that did not happen.
    Unsupported,
    /// Signalling failed or termination could not be confirmed within the
    /// bounded escalation budget; keep the entry and surface an error.
    Failed,
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

    /// Return `true` while the referenced process still exists in the table.
    ///
    /// Uses signal `0`, which performs delivery checks without sending a signal.
    /// A reaped process reports `ESRCH`. A terminated-but-unreaped process
    /// (zombie) still answers signal `0`; distinguishing that case is the
    /// caller's job ([`OwnedTarget::is_alive`]), which pairs this with a `/proc`
    /// state read now that the pidfd has proven the pid is not yet recycled.
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
    confirmed_gone: AtomicBool,
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
            confirmed_gone: AtomicBool::new(pid == 0),
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
        if self.confirmed_gone.load(Ordering::Acquire) || self.pid == 0 {
            return false;
        }
        #[cfg(target_os = "linux")]
        if let Some(pidfd) = &self.pidfd {
            // The pidfd reports a reaped process as gone. A process still in the
            // table may be a terminated-but-unreaped zombie; the pidfd proves
            // `self.pid` is not yet recycled, so the `/proc` state read names this
            // exact process. Treat a zombie as gone for parity with macOS so
            // shutdown confirms termination instead of escalating against a dead
            // process.
            return pidfd.is_alive() && !crate::process_group::pid_is_zombie(self.pid);
        }
        target_alive(self.pid)
    }

    /// Record that this target has been reaped so no later delayed signal can
    /// act on its numeric process identity.
    pub(super) fn mark_reaped(&self) {
        self.confirmed_gone.store(true, Ordering::Release);
    }

    /// Whether termination or a normal waiter has confirmed this target gone.
    pub(super) fn confirmed_gone(&self) -> bool {
        self.confirmed_gone.load(Ordering::Acquire)
    }

    /// Send `signal` to the target's leader (reuse-proof on Linux) and, when
    /// descendant termination is requested, to its process group (best-effort).
    ///
    /// Returns `true` when the signal was delivered or the target has already
    /// exited. The group signal is gated on the *group* still being non-empty
    /// (`kill(-pgid, 0)`), not on the leader still being alive: POSIX keeps a
    /// process-group id reserved while any member survives, so a non-empty probe
    /// names the original group even after the leader itself has exited, which is
    /// what lets escalation reach a descendant that outlived its group leader.
    /// The probe and the follow-up `-pgid` signal are not atomic, so this stays
    /// best-effort: if the group empties in between, a recycled id could reach an
    /// unrelated group. No portable primitive signals a group by a stable handle
    /// (a cgroup/job object would be required), so that residual window cannot be
    /// closed here; the leader signal stays exact through the pidfd.
    fn signal(&self, signal: ProcessSignal) -> bool {
        if self.pid == 0 || self.confirmed_gone() {
            return false;
        }
        #[cfg(target_os = "linux")]
        if let Some(pidfd) = &self.pidfd {
            if self.process_group && group_alive(self.pid) {
                // The group still has a member, so at this instant its id names
                // the original group. Signal the whole group first — before the
                // leader-specific signal — so descendants are reached even when
                // the leader has already exited. This probe-then-signal is not
                // atomic, so it is best-effort: an id recycled in the gap could be
                // signalled instead. The leader signal below stays exact via the
                // pidfd.
                let _ = signal_target(self.pid, signal, true);
            }
            // Then signal the exact leader through its reuse-proof pidfd.
            return pidfd.send(signal.as_raw());
        }
        // Non-Linux Unix (and Linux without pidfd support): best-effort by pid /
        // process-group id, gated on the group liveness probe for the group case.
        #[cfg(unix)]
        {
            if self.process_group {
                if group_alive(self.pid) {
                    return signal_target(self.pid, signal, true);
                }
                // The group is empty; fall back to signalling the leader pid
                // directly rather than a possibly-recycled group id.
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
    /// escalation enabled a still-alive target is force-killed. On a platform
    /// with no signalling primitive the target can still be killed if this
    /// [`OwnedTarget`] owns its child handle; otherwise termination cannot even
    /// be attempted and the target is reported [`Unsupported`] so the caller
    /// surfaces the platform limitation rather than claiming a false success.
    ///
    /// [`Unsupported`]: TargetOutcome::Unsupported
    pub(super) async fn terminate(&self, policy: LifecyclePolicy) -> TargetOutcome {
        if self.pid == 0 || !cfg!(unix) {
            // No reuse-proof signalling identity (`pid == 0`) or no signalling
            // primitive on this platform. Resolve the target through its owned
            // child handle instead, honouring `kill_after_grace(false)`.
            return self.terminate_without_signalling(policy).await;
        }
        if !self.signal(ProcessSignal::Terminate) && self.is_alive() {
            return TargetOutcome::Failed;
        }
        if self.wait_until_gone(policy.grace_period).await {
            self.reap().await;
            self.mark_reaped();
            return TargetOutcome::Terminated;
        }
        if !policy.kill_after_grace {
            return TargetOutcome::Survived;
        }
        if !self.signal(ProcessSignal::Kill) && self.is_alive() {
            return TargetOutcome::Failed;
        }
        // `SIGKILL` was delivered to the whole group and is guaranteed lethal. A
        // bounded wait lets the subtree drain for determinism, but timing out only
        // means an external reaper (init/a subreaper) has not yet collected the
        // orphaned descendant zombies — which we neither own nor can reap. The
        // group is terminated regardless, so this must not be reported as a
        // failure to confirm.
        self.wait_until_gone(policy.grace_period).await;
        self.reap().await;
        self.mark_reaped();
        TargetOutcome::Terminated
    }

    /// Whether this target may be unregistered: the leader is gone and, for a
    /// group target, the whole process group is empty.
    ///
    /// This is the group-aware liveness verdict that separates *leader* death
    /// from *subtree* death. A leader-only view ([`is_alive`](Self::is_alive))
    /// reports gone the moment the direct child exits; but a group target owns a
    /// subtree, so it is only truly gone once the last member of its process
    /// group has left. The leader pidfd/pid stays the reuse-proof handle for the
    /// one process we parented, while [`group_alive`] decides the subtree.
    fn is_gone(&self) -> bool {
        if self.confirmed_gone() {
            return true;
        }
        if self.is_alive() {
            return false;
        }
        // The leader is gone; a group target is only done once its subtree is
        // empty. `group_alive` is `false` on non-group targets and non-Unix, so
        // this reduces to leader liveness there.
        !(self.process_group && group_alive(self.pid))
    }

    /// Reap the owned leader child handle if it has already exited, without
    /// marking the whole target gone.
    ///
    /// A group target may still have live descendants after its leader exits, so
    /// reaping the leader here keeps it from lingering as a zombie (which would
    /// otherwise keep the group probe reporting the subtree alive) while leaving
    /// the group-empty verdict to [`is_gone`](Self::is_gone).
    fn try_reap_leader(&self) {
        if let Some(child) = self.child.lock().as_mut() {
            let _ = child.try_reap();
        }
    }

    async fn wait_until_gone(&self, budget: Duration) -> bool {
        let deadline = tokio::time::Instant::now() + budget;
        loop {
            // Reap the leader as soon as it exits so a leader zombie never keeps
            // the group probe reporting the subtree alive; the group-empty check
            // then governs a group target's final verdict.
            self.try_reap_leader();
            if self.is_gone() {
                self.mark_reaped();
                return true;
            }
            let now = tokio::time::Instant::now();
            if now >= deadline {
                return false;
            }
            tokio::time::sleep(
                Duration::from_millis(10).min(deadline.saturating_duration_since(now)),
            )
            .await;
        }
    }

    /// Whether this target still holds an owned child handle.
    fn owns_child(&self) -> bool {
        self.child.lock().is_some()
    }

    /// Reap the owned child if it already exited on its own, returning `true`
    /// when one was found already-exited and reaped.
    ///
    /// Unlike [`force_reap_owned_child`](Self::force_reap_owned_child) this never
    /// kills a live child, so it is safe on the no-signalling path under
    /// `kill_after_grace(false)`.
    fn reap_if_owned_child_exited(&self) -> bool {
        let mut guard = self.child.lock();
        if let Some(child) = guard.as_mut()
            && child.try_reap().is_some()
        {
            *guard = None;
            drop(guard);
            self.mark_reaped();
            return true;
        }
        false
    }

    /// Resolve a target that has no reuse-proof signalling identity — either
    /// `pid == 0` or a platform without a `kill(2)` primitive — through its owned
    /// child handle.
    ///
    /// An already-exited child is reaped regardless of policy. A still-live owned
    /// child is force-killed only when escalation is enabled; with
    /// `kill_after_grace(false)` it is left registered and reported
    /// [`Survived`](TargetOutcome::Survived). A target with neither a signalling
    /// primitive nor an owned handle cannot even attempt termination and is
    /// reported [`Unsupported`](TargetOutcome::Unsupported).
    async fn terminate_without_signalling(&self, policy: LifecyclePolicy) -> TargetOutcome {
        if self.reap_if_owned_child_exited() {
            return TargetOutcome::Terminated;
        }
        if !self.owns_child() {
            return TargetOutcome::Unsupported;
        }
        if !policy.kill_after_grace {
            return TargetOutcome::Survived;
        }
        if self.force_reap_owned_child().await {
            TargetOutcome::Terminated
        } else {
            TargetOutcome::Unsupported
        }
    }

    /// Kill and reap an owned child handle, returning `true` when one was owned.
    ///
    /// Used on platforms without a signalling primitive: `Child::kill`/`wait`
    /// work regardless of platform, so a relinquished child can still be torn
    /// down through the handle even when there is no `kill(2)`.
    async fn force_reap_owned_child(&self) -> bool {
        let child = self.child.lock().take();
        if let Some(mut child) = child {
            child.start_kill();
            child.reap().await;
            self.mark_reaped();
            true
        } else {
            false
        }
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
            self.mark_reaped();
        }
    }

    /// Force-kill and reap synchronously, for the supervisor's drop backstop.
    ///
    /// Signals the target, then reaps an owned child within a bounded budget so a
    /// synchronous [`Drop`] never blocks indefinitely on a process that ignores
    /// signalling.
    pub(super) fn kill_blocking(&self) {
        if self.pid != 0 {
            self.signal(ProcessSignal::Kill);
        }
        let child = self.child.lock().take();
        if let Some(mut child) = child {
            child.start_kill();
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if child.try_reap().is_some() {
                    self.mark_reaped();
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
    use tokio::io::AsyncReadExt;

    /// Spawn a process-group leader that backgrounds a `SIGTERM`-ignoring
    /// descendant and then exits cleanly, so the group outlives its leader.
    ///
    /// The handshake is race-free: the descendant installs its `trap` and only
    /// then marks a temp file, the leader waits for that mark before printing
    /// `ready` and exiting, and the returned child is not resolved until `ready`
    /// is read. The descendant redirects its own stdio to `/dev/null`, so it
    /// never holds the leader's `ready` pipe open.
    async fn spawn_stubborn_group() -> (tokio::process::Child, u32) {
        let script = "F=$(mktemp); \
             (trap '' TERM; echo 1 > \"$F\"; while :; do sleep 30; done) >/dev/null 2>&1 & \
             until [ -s \"$F\" ]; do :; done; rm -f \"$F\"; printf ready; exit 0";
        let mut command = tokio::process::Command::new("/bin/sh");
        command
            .args(["-c", script])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        crate::process_group::isolate_async(&mut command);
        let mut child = command.spawn().expect("group-leader child spawns");
        let pid = child.id().expect("live pid");
        let mut ready = [0_u8; 5];
        child
            .stdout
            .take()
            .expect("stdout")
            .read_exact(&mut ready)
            .await
            .expect("read readiness");
        assert_eq!(&ready, b"ready");
        (child, pid)
    }

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
        let mut child = tokio::process::Command::new("/bin/sh")
            .args(["-c", "trap '' TERM; printf ready; sleep 30"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("child spawns");
        let mut ready = [0_u8; 5];
        child
            .stdout
            .as_mut()
            .expect("stdout")
            .read_exact(&mut ready)
            .await
            .expect("read readiness");
        assert_eq!(&ready, b"ready");
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

    /// Without a reuse-proof signalling identity (`pid == 0`, mirroring platforms
    /// that lack a `kill(2)` primitive), `kill_after_grace(false)` must retain a
    /// live owned child and report [`TargetOutcome::Survived`] rather than
    /// force-killing it through the handle.
    #[tokio::test]
    async fn no_signalling_target_survives_without_force_kill() {
        let child = tokio::process::Command::new("/bin/sh")
            .args(["-c", "sleep 30"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("child spawns");
        let target = OwnedTarget::new(0, false);
        target.attach_child(OwnedChild::Tokio(child));

        let policy = LifecyclePolicy {
            kill_after_grace: false,
            ..LifecyclePolicy::default()
        };
        let outcome = target.terminate(policy).await;

        assert_eq!(
            outcome,
            TargetOutcome::Survived,
            "no signalling primitive and escalation disabled: keep the child"
        );

        // Escalation enabled tears the same still-owned child down through its
        // handle, confirming ownership was retained rather than dropped.
        let killed = LifecyclePolicy {
            kill_after_grace: true,
            ..LifecyclePolicy::default()
        };
        assert_eq!(target.terminate(killed).await, TargetOutcome::Terminated);
    }

    /// A group leader that exits cleanly while a backgrounded descendant keeps
    /// its process group alive — and the descendant ignores `SIGTERM` — is *not*
    /// reported terminated on leader exit: `SIGKILL` escalation reaches the
    /// surviving group and the target is only terminated once the whole subtree
    /// is gone.
    #[tokio::test]
    async fn terminate_escalates_to_a_group_that_outlives_its_leader() {
        let (child, pid) = spawn_stubborn_group().await;
        let target = OwnedTarget::new(pid, true);
        target.attach_child(OwnedChild::Tokio(child));

        let policy = LifecyclePolicy::default().with_grace_period(Duration::from_millis(50));
        let outcome = target.terminate(policy).await;

        assert_eq!(
            outcome,
            TargetOutcome::Terminated,
            "the surviving group is escalated to and reaped"
        );
        assert!(
            !group_alive(pid),
            "the whole process group is gone, not just the leader"
        );
    }

    /// With `kill_after_grace = false` a `SIGTERM`-ignoring group that outlives
    /// its leader is a deliberate survivor: it is never force-killed and stays
    /// owned so a later shutdown or the drop backstop can reap the whole group.
    #[tokio::test]
    async fn group_that_outlives_its_leader_survives_without_force_kill() {
        let (child, pid) = spawn_stubborn_group().await;
        let target = OwnedTarget::new(pid, true);
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
            "the surviving group is retained, not force-killed"
        );
        assert!(group_alive(pid), "the descendant is still running");

        // The backstop force-kills the whole group.
        target.kill_blocking();
        for _ in 0..500 {
            if !group_alive(pid) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(!group_alive(pid), "backstop reaps the surviving group");
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
