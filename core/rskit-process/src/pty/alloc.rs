//! Pseudoterminal allocation via `openpty`.

use std::os::unix::io::{FromRawFd, OwnedFd, RawFd};

use crate::{AppError, AppResult, ErrorCode};

use super::size::PtySize;

/// A freshly allocated master/slave pseudoterminal pair.
///
/// The master is the side the parent reads rendered output from and writes
/// input to; the slave becomes the child's controlling terminal (its stdin,
/// stdout, and stderr). Both fds are owned so they close deterministically on
/// drop — no fd is leaked on an error path.
pub(crate) struct PtyPair {
    /// Parent-side fd: read child output, write child input. Non-blocking.
    pub(crate) master: OwnedFd,
    /// Child-side fd wired to the child's stdio and made its controlling tty.
    pub(crate) slave: OwnedFd,
}

/// Allocate a pseudoterminal pair sized to `size`.
///
/// The master is set non-blocking (for async readiness-driven reads) and
/// close-on-exec (so the child never inherits it). The slave intentionally
/// keeps its exec-inheritable, blocking defaults: it is dup'd onto the child's
/// stdio by the spawn machinery.
///
/// # Errors
/// Returns [`ErrorCode::Internal`] when the platform `openpty` call fails
/// (for example under fd exhaustion).
pub(crate) fn open_pty(size: PtySize) -> AppResult<PtyPair> {
    let mut master_fd: RawFd = -1;
    let mut slave_fd: RawFd = -1;
    let mut winsize = size.to_winsize();

    // SAFETY: `openpty` writes two valid fds through the out-pointers and the
    // window size through `winp`; all three point at live, uniquely-borrowed
    // locals. A non-zero return leaves the fds untouched, which we map to an
    // error.
    let result = unsafe {
        libc::openpty(
            &raw mut master_fd,
            &raw mut slave_fd,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &raw mut winsize,
        )
    };
    if result != 0 {
        return Err(AppError::new(
            ErrorCode::Internal,
            format!(
                "failed to allocate pseudoterminal: {}",
                std::io::Error::last_os_error()
            ),
        ));
    }

    // SAFETY: `openpty` returned success, so both fds are freshly opened and
    // owned exclusively by us; wrapping them transfers that ownership to
    // `OwnedFd`, which closes them on drop.
    let master = unsafe { OwnedFd::from_raw_fd(master_fd) };
    let slave = unsafe { OwnedFd::from_raw_fd(slave_fd) };

    set_nonblocking(&master)?;
    set_cloexec(&master)?;

    Ok(PtyPair { master, slave })
}

/// Put `fd` into non-blocking mode so reads return `EAGAIN` instead of parking
/// the runtime thread.
fn set_nonblocking(fd: &OwnedFd) -> AppResult<()> {
    use std::os::unix::io::AsRawFd;
    let raw = fd.as_raw_fd();
    // SAFETY: `F_GETFL`/`F_SETFL` read and write the descriptor flags of a live,
    // owned fd; the result is checked before use.
    let flags = unsafe { libc::fcntl(raw, libc::F_GETFL) };
    if flags < 0 {
        return Err(fcntl_error("F_GETFL"));
    }
    // SAFETY: as above; setting `O_NONBLOCK` alongside the existing flags.
    if unsafe { libc::fcntl(raw, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(fcntl_error("F_SETFL O_NONBLOCK"));
    }
    Ok(())
}

/// Mark `fd` close-on-exec so spawned children never inherit the master side.
fn set_cloexec(fd: &OwnedFd) -> AppResult<()> {
    use std::os::unix::io::AsRawFd;
    let raw = fd.as_raw_fd();
    // SAFETY: `F_GETFD`/`F_SETFD` read and write the descriptor flags of a live,
    // owned fd; the result is checked before use.
    let flags = unsafe { libc::fcntl(raw, libc::F_GETFD) };
    if flags < 0 {
        return Err(fcntl_error("F_GETFD"));
    }
    // SAFETY: as above; setting `FD_CLOEXEC` alongside the existing flags.
    if unsafe { libc::fcntl(raw, libc::F_SETFD, flags | libc::FD_CLOEXEC) } < 0 {
        return Err(fcntl_error("F_SETFD FD_CLOEXEC"));
    }
    Ok(())
}

fn fcntl_error(op: &str) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!(
            "failed to configure pseudoterminal master ({op}): {}",
            std::io::Error::last_os_error()
        ),
    )
}

#[cfg(test)]
mod tests {
    use std::os::unix::io::AsRawFd;

    use super::*;

    #[test]
    fn open_pty_returns_distinct_live_fds() {
        let pair = open_pty(PtySize::new(30, 100)).expect("openpty");
        assert!(pair.master.as_raw_fd() >= 0);
        assert!(pair.slave.as_raw_fd() >= 0);
        assert_ne!(pair.master.as_raw_fd(), pair.slave.as_raw_fd());
    }

    #[test]
    fn master_is_nonblocking_and_cloexec() {
        let pair = open_pty(PtySize::default()).expect("openpty");
        let raw = pair.master.as_raw_fd();
        // SAFETY: reading the flags of the owned master fd for assertions.
        let status = unsafe { libc::fcntl(raw, libc::F_GETFL) };
        assert!(
            status & libc::O_NONBLOCK != 0,
            "master must be non-blocking"
        );
        // SAFETY: as above, descriptor flags.
        let descriptor = unsafe { libc::fcntl(raw, libc::F_GETFD) };
        assert!(
            descriptor & libc::FD_CLOEXEC != 0,
            "master must be close-on-exec"
        );
    }
}
