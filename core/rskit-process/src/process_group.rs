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
    target_alive_inner(pid)
}

/// Whether any process remains in the process group led by `pgid`.
///
/// Probes the whole group with `kill(-pgid, 0)`: it returns `true` while the
/// group has at least one member (or a member owned by another user, reported as
/// `EPERM`) and `false` once the group is empty (`ESRCH`). This is the group
/// analogue of [`target_alive`] for deciding whether a supervised subtree is
/// gone: POSIX keeps a process-group id reserved while the group still has a
/// member, so a non-empty result names the original group at the instant of the
/// probe — never a recycled id. Unlike a Linux pidfd it is portable to every
/// Unix, which is why group liveness — as opposed to leader liveness — is
/// modelled through it.
///
/// The reservation guarantee is point-in-time: it makes the *probe* accurate but
/// does not make a later numeric group signal atomic with it. Once the group
/// empties its id can be recycled, so a caller that probes and then signals
/// `-pgid` still races that window — group signalling is best-effort, not
/// reuse-proof, because no portable primitive signals a group by a stable handle
/// (a Linux cgroup or a job object would be required).
///
/// A group member that has exited but not yet been reaped (a zombie) still
/// answers the probe until it is reaped; this only makes the probe conservative
/// (it briefly reports the group alive), never falsely empty, so a bounded poll
/// converges once the last member is reaped.
pub(crate) fn group_alive(pgid: u32) -> bool {
    if pgid == 0 {
        return false;
    }
    #[cfg(unix)]
    {
        let Ok(pgid) = i32::try_from(pgid) else {
            return false;
        };
        // SAFETY: signal 0 performs an existence check without delivering a
        // signal. A negative target names the process group. ESRCH means the
        // group is empty; EPERM means a member exists but is owned by another
        // user, which still counts as alive.
        unsafe {
            if libc::kill(-pgid, 0) == 0 {
                return true;
            }
            std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
        }
    }
    #[cfg(not(unix))]
    {
        let _ = pgid;
        false
    }
}

/// Whether the process group led by `pgid` still has a *genuinely live* member —
/// one that has not exited.
///
/// [`group_alive`] is deliberately conservative: a member that has exited but
/// not yet been reaped (a zombie) keeps answering its probe until reaped, so it
/// briefly reports a group of only-dead members as alive. That is the right
/// verdict for escalation timing, but not for *confirming termination*: once
/// `SIGKILL` has been delivered, a lingering zombie is already dead and merely
/// awaiting an external reaper, whereas a member still running (or stuck in
/// uninterruptible sleep) genuinely survives. This enumerates the group and
/// returns `true` only when at least one member is not a zombie, so shutdown can
/// tell "confirmed gone — only zombies remain" from "not gone — a live member
/// persists". On non-Unix targets there are no process groups, so it is `false`.
pub(crate) fn group_has_live_member(pgid: u32) -> bool {
    if pgid == 0 {
        return false;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let target = pgid.to_string();
        let Ok(dir) = std::fs::read_dir("/proc") else {
            return false;
        };
        for entry in dir.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if name.parse::<u32>().is_err() {
                continue;
            }
            let Ok(stat) = std::fs::read_to_string(format!("/proc/{name}/stat")) else {
                continue;
            };
            // `/proc/<pid>/stat` is "pid (comm) state ppid pgrp ...". `comm` may
            // contain spaces and parentheses, so the fields after the final ')'
            // are: state, ppid, pgrp, ...
            let Some((_, after_comm)) = stat.rsplit_once(')') else {
                continue;
            };
            let mut fields = after_comm.split_whitespace();
            let state = fields.next();
            let _ppid = fields.next();
            if fields.next() == Some(target.as_str()) && state != Some("Z") {
                return true;
            }
        }
        false
    }
    #[cfg(target_os = "macos")]
    {
        // libproc's `PROC_PGRP_ONLY` lists the pids in a process group; the
        // per-pid zombie-aware [`target_alive`] then decides which are genuinely
        // live. (`proc_listpgrppids` is unreliable here, returning nothing for
        // valid groups, so the underlying `proc_listpids` is used directly.)
        const PROC_PGRP_ONLY: u32 = 2;

        #[link(name = "proc")]
        unsafe extern "C" {
            fn proc_listpids(
                r#type: u32,
                typeinfo: u32,
                buffer: *mut libc::c_void,
                buffersize: libc::c_int,
            ) -> libc::c_int;
        }

        // SAFETY: a null buffer with size 0 asks libproc for the byte count
        // needed to list the group's pids without writing anything.
        let needed = unsafe { proc_listpids(PROC_PGRP_ONLY, pgid, std::ptr::null_mut(), 0) };
        let Ok(needed) = usize::try_from(needed) else {
            return false;
        };
        let count = needed / std::mem::size_of::<libc::pid_t>();
        if count == 0 {
            return false;
        }
        let mut pids = vec![0 as libc::pid_t; count];
        let Ok(buffer_size) = libc::c_int::try_from(std::mem::size_of_val(pids.as_slice())) else {
            return false;
        };
        // SAFETY: `pids` is a writable buffer of exactly `buffer_size` bytes.
        let filled =
            unsafe { proc_listpids(PROC_PGRP_ONLY, pgid, pids.as_mut_ptr().cast(), buffer_size) };
        let Ok(filled) = usize::try_from(filled) else {
            return false;
        };
        let filled = (filled / std::mem::size_of::<libc::pid_t>()).min(pids.len());
        pids.iter()
            .take(filled)
            .any(|&pid| u32::try_from(pid).is_ok_and(|pid| pid != 0 && target_alive(pid)))
    }
    #[cfg(not(unix))]
    {
        false
    }
}

fn target_alive_inner(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(target_os = "macos")]
    {
        return macos_target_alive(pid);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let Ok(signed_pid) = i32::try_from(pid) else {
            return false;
        };
        // SAFETY: signal 0 performs an existence check without delivering a signal.
        // EPERM means the process exists but is owned by another user.
        unsafe {
            if libc::kill(signed_pid, 0) == 0 {
                // A terminated-but-unreaped process (a zombie) still answers
                // `kill(pid, 0)` yet has already exited; treat it as gone so
                // Linux matches the macOS `SZOMB` check and shutdown can confirm
                // termination instead of escalating against a dead process.
                return !pid_is_zombie(pid);
            }
            std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
        }
    }

    #[cfg(target_os = "macos")]
    fn macos_target_alive(pid: u32) -> bool {
        const PROC_PIDTBSDINFO: libc::c_int = 3;
        const SZOMB: u32 = 5;

        #[repr(C)]
        struct ProcBsdInfo {
            flags: u32,
            status: u32,
            xstatus: u32,
            pid: u32,
            ppid: u32,
            uid: libc::uid_t,
            gid: libc::gid_t,
            ruid: libc::uid_t,
            rgid: libc::gid_t,
            svuid: libc::uid_t,
            svgid: libc::gid_t,
            reserved: u32,
            command: [libc::c_char; 16],
            name: [libc::c_char; 32],
            file_count: u32,
            process_group: u32,
            job_control_count: u32,
            controlling_terminal: u32,
            terminal_process_group: u32,
            nice: i32,
            start_seconds: u64,
            start_microseconds: u64,
        }

        #[link(name = "proc")]
        unsafe extern "C" {
            fn proc_pidinfo(
                pid: libc::c_int,
                flavor: libc::c_int,
                arg: u64,
                buffer: *mut libc::c_void,
                buffer_size: libc::c_int,
            ) -> libc::c_int;
        }

        let Ok(pid) = i32::try_from(pid) else {
            return false;
        };
        let mut process = std::mem::MaybeUninit::<ProcBsdInfo>::uninit();
        let Ok(buffer_size) = libc::c_int::try_from(std::mem::size_of::<ProcBsdInfo>()) else {
            return unix_target_alive(pid);
        };
        // SAFETY: `process` is a writable buffer of the exact declared size for
        // `PROC_PIDTBSDINFO`; the call initializes it on a full-size result.
        let bytes = unsafe {
            proc_pidinfo(
                pid,
                PROC_PIDTBSDINFO,
                0,
                process.as_mut_ptr().cast(),
                buffer_size,
            )
        };
        if bytes == 0 {
            return std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM);
        }
        if bytes != buffer_size {
            return unix_target_alive(pid);
        }
        // SAFETY: a full-size result initialized the complete structure.
        let process = unsafe { process.assume_init() };
        process.status != SZOMB
    }

    #[cfg(target_os = "macos")]
    fn unix_target_alive(pid: libc::pid_t) -> bool {
        // SAFETY: signal 0 performs an existence check without delivering a signal.
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

/// Whether `pid` is a terminated-but-unreaped process (a zombie).
///
/// A zombie still answers existence probes (`kill(pid, 0)` and a `pidfd` signal
/// `0`) until it is reaped, yet it has already exited. Reading its `/proc` state
/// lets the Linux liveness probes treat it as gone, matching the macOS `SZOMB`
/// check so shutdown confirms termination instead of escalating against a dead
/// process. On Unix targets without a Linux-style `/proc` the read fails and the
/// probe conservatively reports "not a zombie".
#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) fn pid_is_zombie(pid: u32) -> bool {
    let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return false;
    };
    // `/proc/<pid>/stat` is "pid (comm) state ...". `comm` can itself contain
    // spaces and parentheses, so the state field is the first token after the
    // final ')'.
    let Some((_, after_comm)) = stat.rsplit_once(')') else {
        return false;
    };
    after_comm.trim_start().starts_with('Z')
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
        assert!(!group_alive(0));
    }

    #[cfg(unix)]
    #[test]
    fn group_alive_tracks_the_whole_group_not_just_the_leader() {
        // The leader exits immediately but backgrounds a descendant that keeps
        // the process group non-empty. A leader-only liveness view would call the
        // group gone the moment the leader exits; the group probe must not. The
        // descendant's stdio is detached so it does not hold the leader's pipes
        // open past the leader's own exit.
        let mut command = StdCommand::new("/bin/sh");
        command
            .args(["-c", "sleep 30 >/dev/null 2>&1 & exit 0"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        isolate(&mut command);
        let mut leader = command.spawn().expect("group-leader child spawns");
        let pgid = leader.id();

        // Reap the leader so it is no longer a group member; the backgrounded
        // descendant still holds the group open.
        let status = leader.wait().expect("leader reaped");
        assert!(status.success(), "leader exits cleanly");
        assert!(
            group_alive(pgid),
            "the group is still alive through its surviving descendant"
        );

        // Terminating the group reaches the descendant even though the leader is
        // already gone, and the group empties once it exits.
        assert!(kill(pgid), "killing the surviving group succeeds");
        for _ in 0..500 {
            if !group_alive(pgid) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            !group_alive(pgid),
            "the group is empty once its last member is gone"
        );
    }

    #[cfg(unix)]
    #[test]
    fn group_has_live_member_distinguishes_live_members_from_zombies() {
        // A group leader that backgrounds a live descendant and then exits leaves
        // the group with a live member: `group_has_live_member` must see it.
        let mut command = StdCommand::new("/bin/sh");
        command
            .args(["-c", "sleep 30 >/dev/null 2>&1 & exit 0"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        isolate(&mut command);
        let mut leader = command.spawn().expect("group-leader child spawns");
        let pgid = leader.id();
        leader.wait().expect("leader reaped");

        assert!(
            group_has_live_member(pgid),
            "a live backgrounded descendant is a live group member"
        );

        // Kill the group; once only unreaped zombies (or nothing) remain, no
        // member is genuinely live even while `group_alive` may still be
        // conservatively true.
        assert!(kill(pgid), "killing the group succeeds");
        for _ in 0..500 {
            if !group_has_live_member(pgid) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            !group_has_live_member(pgid),
            "a killed group has no genuinely live member"
        );
    }

    #[test]
    fn group_has_live_member_rejects_zero_pgid() {
        assert!(!group_has_live_member(0));
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
