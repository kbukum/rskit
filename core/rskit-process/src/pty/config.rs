//! PTY-backed I/O mode configuration.

use crate::command::{InputPolicy, OutputPolicy};
use crate::runner::OutputObserver;

use super::size::PtySize;

/// Pseudoterminal-backed I/O mode.
///
/// The child runs attached to a real controlling terminal, so it renders output
/// exactly as it would in an interactive shell — colors, progress bars, and
/// tty-gated formatting are preserved. Unlike the pipe-backed modes, a PTY has a
/// single stream: the child's stdout and stderr are merged in emission order.
///
/// The merged stream is delivered through the observer's **stdout** callbacks
/// and, when the output policy captures stdout, retained as the result's stdout.
/// The child never writes to the result's stderr in this mode; the only bytes
/// that can appear there are synthetic termination diagnostics injected by the
/// lifecycle layer (for example a note that the child was killed after a timeout
/// or cancellation).
#[derive(Clone, Default)]
pub struct PtyIo {
    /// Standard input policy. Only [`InputPolicy::Closed`] and
    /// [`InputPolicy::Bytes`] are supported; inherited stdin is rejected.
    ///
    /// Terminal stdin cannot be half-closed, so [`InputPolicy::Closed`] does not
    /// deliver a pipe-style EOF here: it simply means no bytes are ever written
    /// to the child's terminal (the child keeps reading from a live tty that
    /// stays open until the PTY is torn down), not that the child sees stdin
    /// closed. Likewise [`InputPolicy::Bytes`] are delivered as terminal input
    /// (as if typed) through the PTY's line discipline, so a reader returns on a
    /// newline rather than on the writer closing.
    pub input: InputPolicy,
    /// Capture policy for the merged output stream (applied to stdout).
    pub output: OutputPolicy,
    /// Observer callbacks for the merged output stream (delivered via stdout).
    pub observer: OutputObserver,
    /// Window size advertised to the child through the PTY.
    pub size: PtySize,
}

impl std::fmt::Debug for PtyIo {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PtyIo")
            .field("input", &self.input)
            .field("output", &self.output)
            .field("observer", &self.observer)
            .field("size", &self.size)
            .finish()
    }
}

impl PtyIo {
    /// Create a PTY mode that observes the merged stream through `observer`.
    #[must_use]
    pub fn new(observer: OutputObserver) -> Self {
        Self {
            observer,
            ..Self::default()
        }
    }

    /// Set the stdin policy.
    #[must_use]
    pub fn with_input(mut self, input: InputPolicy) -> Self {
        self.input = input;
        self
    }

    /// Set the capture policy for the merged output stream.
    #[must_use]
    pub fn with_output(mut self, output: OutputPolicy) -> Self {
        self.output = output;
        self
    }

    /// Set the window size advertised to the child.
    #[must_use]
    pub fn with_size(mut self, size: PtySize) -> Self {
        self.size = size;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builders_update_fields() {
        let io = PtyIo::new(OutputObserver::new())
            .with_input(InputPolicy::Bytes(b"y\n".to_vec()))
            .with_output(OutputPolicy::observe_only())
            .with_size(PtySize::new(50, 200));
        assert_eq!(io.input, InputPolicy::Bytes(b"y\n".to_vec()));
        assert!(!io.output.capture_stdout);
        assert_eq!(io.size, PtySize::new(50, 200));
    }

    #[test]
    fn debug_reports_observer_presence() {
        let io = PtyIo::new(OutputObserver::new().with_stdout_bytes(|_| {}));
        let rendered = format!("{io:?}");
        assert!(rendered.contains("PtyIo"));
        assert!(rendered.contains("stdout_bytes: true"));
    }
}
