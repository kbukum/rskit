use std::num::NonZeroI32;
use std::time::Duration;

/// Operating-system signal arm that can begin or escalate shutdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ShutdownSignal {
    /// Ctrl+C / SIGINT.
    Interrupt,
    /// SIGTERM on Unix platforms.
    #[cfg(unix)]
    Terminate,
    /// SIGHUP on Unix platforms.
    #[cfg(unix)]
    Hangup,
    /// Windows console close event.
    #[cfg(windows)]
    Close,
    /// A caller-supplied Unix signal number for platform-specific extensions.
    #[cfg(unix)]
    UnixRaw(i32),
}

impl ShutdownSignal {
    /// Return the Ctrl+C / SIGINT shutdown signal.
    #[must_use]
    pub const fn interrupt() -> Self {
        Self::Interrupt
    }

    /// Return the SIGTERM shutdown signal.
    #[cfg(unix)]
    #[must_use]
    pub const fn terminate() -> Self {
        Self::Terminate
    }

    /// Return the SIGHUP shutdown signal.
    #[cfg(unix)]
    #[must_use]
    pub const fn hangup() -> Self {
        Self::Hangup
    }

    /// Return a caller-supplied Unix signal number.
    #[cfg(unix)]
    #[must_use]
    pub const fn unix_raw(signal: i32) -> Self {
        Self::UnixRaw(signal)
    }

    /// Return the Windows console close shutdown signal.
    #[cfg(windows)]
    #[must_use]
    pub const fn close() -> Self {
        Self::Close
    }
}

/// Declarative shutdown behavior used to install a [`ShutdownController`](crate::ShutdownController).
#[derive(Debug, Clone)]
pub struct ShutdownPolicy {
    pub(crate) signals: Vec<ShutdownSignal>,
    pub(crate) drain_deadline: Option<Duration>,
    pub(crate) second_signal_exit_code: NonZeroI32,
}

impl Default for ShutdownPolicy {
    fn default() -> Self {
        Self {
            signals: default_signals(),
            drain_deadline: None,
            second_signal_exit_code: default_second_signal_exit_code(),
        }
    }
}

impl ShutdownPolicy {
    /// Replace the signal set that begins and escalates shutdown.
    #[must_use]
    pub fn with_signals<I>(mut self, signals: I) -> Self
    where
        I: IntoIterator<Item = ShutdownSignal>,
    {
        self.signals = signals.into_iter().collect();
        self
    }

    /// Add one signal to the configured shutdown signal set.
    #[must_use]
    pub fn with_signal(mut self, signal: ShutdownSignal) -> Self {
        self.signals.push(signal);
        self
    }

    /// Set the cooperative drain deadline after the first signal.
    #[must_use]
    pub const fn with_drain_deadline(mut self, deadline: Duration) -> Self {
        self.drain_deadline = Some(deadline);
        self
    }

    /// Set the non-zero process exit code used for a second signal or expired drain deadline.
    #[must_use]
    pub const fn with_second_signal_exit_code(mut self, code: NonZeroI32) -> Self {
        self.second_signal_exit_code = code;
        self
    }

    /// Return the configured shutdown signal set.
    #[must_use]
    pub fn signals(&self) -> &[ShutdownSignal] {
        &self.signals
    }

    /// Return the optional cooperative drain deadline.
    #[must_use]
    pub const fn drain_deadline(&self) -> Option<Duration> {
        self.drain_deadline
    }

    /// Return the non-zero process exit code used for forced shutdown.
    #[must_use]
    pub const fn second_signal_exit_code(&self) -> NonZeroI32 {
        self.second_signal_exit_code
    }
}

#[cfg(unix)]
fn default_signals() -> Vec<ShutdownSignal> {
    vec![
        ShutdownSignal::Interrupt,
        ShutdownSignal::Terminate,
        ShutdownSignal::Hangup,
    ]
}

#[cfg(windows)]
fn default_signals() -> Vec<ShutdownSignal> {
    vec![ShutdownSignal::Interrupt, ShutdownSignal::Close]
}

fn default_second_signal_exit_code() -> NonZeroI32 {
    NonZeroI32::new(130).unwrap_or(NonZeroI32::MIN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_policy_uses_platform_signal_set() {
        let policy = ShutdownPolicy::default();

        #[cfg(unix)]
        assert_eq!(
            policy.signals(),
            &[
                ShutdownSignal::Interrupt,
                ShutdownSignal::Terminate,
                ShutdownSignal::Hangup,
            ]
        );

        #[cfg(windows)]
        assert_eq!(
            policy.signals(),
            &[ShutdownSignal::Interrupt, ShutdownSignal::Close]
        );
    }
}
