/// What interaction model a [`Terminal`](super::Terminal) supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    keys: bool,
}

impl Default for Capabilities {
    /// Line-driven, matching the default (cooked-stdio) terminal.
    fn default() -> Self {
        Self::line_driven()
    }
}

impl Capabilities {
    /// A key-driven terminal: reads individual [`Key`](crate::prompt::key::Key)s and can redraw frames,
    /// enabling live arrow-key navigation.
    #[must_use]
    pub const fn key_driven() -> Self {
        Self { keys: true }
    }

    /// A line-driven terminal: reads a whole line at a time (cooked stdio).
    #[must_use]
    pub const fn line_driven() -> Self {
        Self { keys: false }
    }

    /// Whether the terminal reads individual keys (live widgets) rather than whole lines.
    #[must_use]
    pub const fn is_key_driven(self) -> bool {
        self.keys
    }
}
