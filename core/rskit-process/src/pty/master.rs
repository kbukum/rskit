//! Async, readiness-driven access to the pseudoterminal master fd.

use std::io;
use std::os::unix::io::{AsRawFd, OwnedFd, RawFd};
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use tokio::io::unix::AsyncFd;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// Async wrapper over the PTY master fd.
///
/// Reads deliver the child's merged stdout/stderr stream; writes deliver input
/// to the child's terminal. The fd is registered with the reactor so reads and
/// writes are readiness-driven and never block the runtime thread.
///
/// When the child closes the slave side, a master read yields `EIO` on Linux
/// (and, depending on timing, on other platforms); this is the terminal's
/// end-of-stream signal and is surfaced as a normal `Ok(0)` EOF so the reader
/// loop terminates cleanly rather than erroring.
pub(crate) struct PtyMaster {
    inner: AsyncFd<OwnedFd>,
}

impl PtyMaster {
    /// Register the (already non-blocking) master fd with the reactor.
    ///
    /// # Errors
    /// Propagates a reactor-registration failure from [`AsyncFd::new`].
    pub(crate) fn new(master: OwnedFd) -> io::Result<Self> {
        Ok(Self {
            inner: AsyncFd::new(master)?,
        })
    }
}

fn read_fd(fd: RawFd, buf: &mut [u8]) -> io::Result<usize> {
    // SAFETY: `read` writes at most `buf.len()` bytes into `buf`'s valid,
    // uniquely-borrowed backing store and reports the count (or an error).
    let count = unsafe { libc::read(fd, buf.as_mut_ptr().cast::<libc::c_void>(), buf.len()) };
    if count < 0 {
        return Err(io::Error::last_os_error());
    }
    // A non-negative `count` fits `usize` because it is bounded by `buf.len()`.
    Ok(count as usize)
}

fn write_fd(fd: RawFd, buf: &[u8]) -> io::Result<usize> {
    // SAFETY: `write` reads at most `buf.len()` bytes from `buf`'s valid,
    // shared-borrowed backing store and reports the count (or an error).
    let count = unsafe { libc::write(fd, buf.as_ptr().cast::<libc::c_void>(), buf.len()) };
    if count < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(count as usize)
}

impl AsyncRead for PtyMaster {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        loop {
            let mut guard = ready!(self.inner.poll_read_ready(cx))?;
            let unfilled = buf.initialize_unfilled();
            let raw = self.inner.get_ref().as_raw_fd();
            match guard.try_io(|_| read_fd(raw, unfilled)) {
                Ok(Ok(count)) => {
                    buf.advance(count);
                    return Poll::Ready(Ok(()));
                }
                // Slave closed: treat the terminal's `EIO` as a clean EOF.
                Ok(Err(error)) if error.raw_os_error() == Some(libc::EIO) => {
                    return Poll::Ready(Ok(()));
                }
                Ok(Err(error)) => return Poll::Ready(Err(error)),
                // Spurious readiness: the reactor cleared it, so retry.
                Err(_would_block) => continue,
            }
        }
    }
}

impl AsyncWrite for PtyMaster {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        loop {
            let mut guard = ready!(self.inner.poll_write_ready(cx))?;
            let raw = self.inner.get_ref().as_raw_fd();
            match guard.try_io(|_| write_fd(raw, buf)) {
                Ok(result) => return Poll::Ready(result),
                Err(_would_block) => continue,
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        // Terminal writes are unbuffered at this layer; nothing to flush.
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        // Closing the write half is expressed by dropping the fd; there is no
        // half-close on a PTY master, so shutdown is a no-op.
        Poll::Ready(Ok(()))
    }
}
