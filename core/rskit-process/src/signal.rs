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
    #[cfg(unix)]
    pub(crate) const fn as_raw(self) -> i32 {
        match self {
            Self::Interrupt => libc::SIGINT,
            Self::Terminate => libc::SIGTERM,
            Self::Kill => libc::SIGKILL,
        }
    }

    /// Platform signal value placeholder for unsupported platforms.
    #[cfg(not(unix))]
    pub(crate) const fn as_raw(self) -> i32 {
        let _ = self;
        0
    }
}
