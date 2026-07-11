//! Pseudoterminal window size and terminal detection.

use std::os::unix::io::AsRawFd;

/// Window size of a pseudoterminal, in character cells plus optional pixels.
///
/// A child process attached to a PTY reads this through `TIOCGWINSZ` to lay out
/// its output (wrapping, progress bars, table widths). Mirroring the size of the
/// controlling terminal keeps rendered output identical to running the command
/// directly.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct PtySize {
    /// Number of visible rows (character cells).
    pub rows: u16,
    /// Number of visible columns (character cells).
    pub cols: u16,
    /// Pixel width of the terminal, `0` when unknown.
    pub pixel_width: u16,
    /// Pixel height of the terminal, `0` when unknown.
    pub pixel_height: u16,
}

impl Default for PtySize {
    /// The conventional `80x24` fallback used when no terminal size is known.
    fn default() -> Self {
        Self {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        }
    }
}

impl PtySize {
    /// Create a size from an explicit `rows` and `cols`, leaving pixels unset.
    #[must_use]
    pub const fn new(rows: u16, cols: u16) -> Self {
        Self {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        }
    }

    /// Convert to the libc `winsize` used by `openpty`/`TIOCSWINSZ`.
    pub(crate) const fn to_winsize(self) -> libc::winsize {
        libc::winsize {
            ws_row: self.rows,
            ws_col: self.cols,
            ws_xpixel: self.pixel_width,
            ws_ypixel: self.pixel_height,
        }
    }
}

/// Query the window size of the terminal backing `fd`.
///
/// Returns `None` when `fd` is not a terminal (for example a pipe or a file, as
/// under CI or output redirection) or the `TIOCGWINSZ` query fails. Callers use
/// a `None` result to decide that no PTY should be allocated.
#[must_use]
pub fn terminal_size(fd: &impl AsRawFd) -> Option<PtySize> {
    // SAFETY: `winsize` is a plain C struct with no invariants, so an
    // all-zero value is a valid initial state that `ioctl` fully overwrites.
    let mut winsize: libc::winsize = unsafe { std::mem::zeroed() };
    // SAFETY: `TIOCGWINSZ` writes a `winsize` through the provided pointer,
    // which is a valid, uniquely-borrowed local. A non-zero return means the
    // fd is not a terminal, which we surface as `None`.
    let result = unsafe { libc::ioctl(fd.as_raw_fd(), libc::TIOCGWINSZ, &raw mut winsize) };
    if result != 0 {
        return None;
    }
    Some(PtySize {
        rows: winsize.ws_row,
        cols: winsize.ws_col,
        pixel_width: winsize.ws_xpixel,
        pixel_height: winsize.ws_ypixel,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_eighty_by_twenty_four() {
        let size = PtySize::default();
        assert_eq!(size.cols, 80);
        assert_eq!(size.rows, 24);
        assert_eq!(size.pixel_width, 0);
        assert_eq!(size.pixel_height, 0);
    }

    #[test]
    fn new_sets_rows_and_cols_only() {
        let size = PtySize::new(40, 120);
        assert_eq!(size.rows, 40);
        assert_eq!(size.cols, 120);
        assert_eq!(size.pixel_width, 0);
    }

    #[test]
    fn winsize_round_trips_dimensions() {
        let winsize = PtySize {
            rows: 10,
            cols: 20,
            pixel_width: 30,
            pixel_height: 40,
        }
        .to_winsize();
        assert_eq!(winsize.ws_row, 10);
        assert_eq!(winsize.ws_col, 20);
        assert_eq!(winsize.ws_xpixel, 30);
        assert_eq!(winsize.ws_ypixel, 40);
    }

    #[test]
    fn non_terminal_fd_has_no_size() {
        // A pipe read end is never a terminal.
        let (reader, _writer) = std::io::pipe().expect("pipe");
        assert!(terminal_size(&reader).is_none());
    }

    #[test]
    fn pty_slave_reports_configured_terminal_size() {
        let pair = crate::pty::open_pty(PtySize::new(33, 111)).expect("openpty");
        let size = terminal_size(&pair.slave).expect("pty slave has size");

        assert_eq!(size.rows, 33);
        assert_eq!(size.cols, 111);
    }
}
