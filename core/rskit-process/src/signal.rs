//! Internal process signal definitions.

/// Process signal used by subprocess lifecycle helpers.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ProcessSignal {
    /// Graceful interruption signal.
    Interrupt,
    /// Graceful termination signal.
    Terminate,
    /// Forceful termination signal.
    Kill,
}

impl ProcessSignal {
    /// Human-readable POSIX signal name.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Interrupt => "SIGINT",
            Self::Terminate => "SIGTERM",
            Self::Kill => "SIGKILL",
        }
    }

    /// Platform signal value.
    ///
    /// Only defined on Unix, where the process-signaling helpers use it; other
    /// platforms have no `kill(2)`-style signaling and never reference it.
    #[cfg(unix)]
    pub(crate) const fn as_raw(self) -> i32 {
        match self {
            Self::Interrupt => libc::SIGINT,
            Self::Terminate => libc::SIGTERM,
            Self::Kill => libc::SIGKILL,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_names_and_raw_values_are_stable() {
        assert_eq!(ProcessSignal::Interrupt.name(), "SIGINT");
        assert_eq!(ProcessSignal::Terminate.name(), "SIGTERM");
        assert_eq!(ProcessSignal::Kill.name(), "SIGKILL");

        #[cfg(unix)]
        {
            assert_eq!(ProcessSignal::Interrupt.as_raw(), libc::SIGINT);
            assert_eq!(ProcessSignal::Terminate.as_raw(), libc::SIGTERM);
            assert_eq!(ProcessSignal::Kill.as_raw(), libc::SIGKILL);
        }
    }
}
