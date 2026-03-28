//! Progress bar abstractions over `indicatif`.
//!
//! Provides preset styles for common CLI progress patterns.

use indicatif::{
    MultiProgress as IndicatifMultiProgress, ProgressBar as IndicatifBar,
    ProgressStyle as IndicatifStyle,
};
use std::time::Duration;

/// Re-export raw type so binaries can create shared instances for writer integration.
pub use indicatif::MultiProgress as RawMultiProgress;

/// Preset progress bar styles.
pub enum ProgressStyle {
    /// Standard bar with count and percentage.
    Bar,
    /// Spinner for indeterminate operations.
    Spinner,
    /// Download-style with bytes.
    Download,
    /// Finished bar — just shows prefix + message, no bar/timer.
    Finished,
}

impl ProgressStyle {
    fn to_indicatif(&self) -> IndicatifStyle {
        match self {
            ProgressStyle::Bar => IndicatifStyle::with_template(
                "{prefix:.bold} {wide_bar:.cyan/dim} {pos}/{len} {percent}% {elapsed}",
            )
            .unwrap()
            .progress_chars("━╸─"),

            ProgressStyle::Spinner => {
                IndicatifStyle::with_template("{spinner:.green} {prefix} {wide_msg}").unwrap()
            }

            ProgressStyle::Download => IndicatifStyle::with_template(
                "{prefix:.bold} {wide_bar:.green/dim} {bytes}/{total_bytes} {bytes_per_sec} {eta}",
            )
            .unwrap()
            .progress_chars("━╸─"),

            ProgressStyle::Finished => {
                IndicatifStyle::with_template("{prefix} {wide_msg}").unwrap()
            }
        }
    }
}

/// A single progress bar wrapping `indicatif`.
pub struct ProgressBar {
    inner: IndicatifBar,
}

impl ProgressBar {
    /// Create a new progress bar with the given total and style.
    pub fn new(total: u64, style: ProgressStyle) -> Self {
        let bar = IndicatifBar::new(total);
        bar.set_style(style.to_indicatif());
        bar.enable_steady_tick(Duration::from_millis(100));
        Self { inner: bar }
    }

    /// Create a spinner (indeterminate progress).
    pub fn spinner() -> Self {
        let bar = IndicatifBar::new_spinner();
        bar.set_style(ProgressStyle::Spinner.to_indicatif());
        bar.enable_steady_tick(Duration::from_millis(80));
        Self { inner: bar }
    }

    /// Set the prefix text (typically appears before the bar).
    pub fn set_prefix(&self, prefix: impl Into<String>) {
        self.inner.set_prefix(prefix.into());
    }

    /// Change the bar style.
    pub fn set_style(&self, style: ProgressStyle) {
        self.inner.set_style(style.to_indicatif());
    }

    /// Set the message text.
    pub fn set_message(&self, msg: impl Into<String>) {
        self.inner.set_message(msg.into());
    }

    /// Enable steady tick (auto-redraw) at the given interval.
    pub fn enable_steady_tick(&self, interval: Duration) {
        self.inner.enable_steady_tick(interval);
    }

    /// Trigger a redraw without changing position.
    pub fn tick(&self) {
        self.inner.tick();
    }

    /// Set current position.
    pub fn set_position(&self, pos: u64) {
        self.inner.set_position(pos);
    }

    /// Increment position by delta.
    pub fn inc(&self, delta: u64) {
        self.inner.inc(delta);
    }

    /// Set total length.
    pub fn set_length(&self, len: u64) {
        self.inner.set_length(len);
    }

    /// Mark as finished.
    pub fn finish(&self) {
        self.inner.finish();
    }

    /// Mark as finished with a final message (bar stays visible).
    pub fn finish_with_message(&self, msg: impl Into<std::borrow::Cow<'static, str>>) {
        self.inner.finish_with_message(msg);
    }

    /// Reset the bar to its initial state (position 0, not finished).
    pub fn reset(&self) {
        self.inner.reset();
    }

    /// Finish and clear from display.
    pub fn finish_and_clear(&self) {
        self.inner.finish_and_clear();
    }

    /// Access the underlying indicatif bar.
    pub fn inner(&self) -> &IndicatifBar {
        &self.inner
    }
}

/// Multi-progress bar manager for concurrent operations.
pub struct MultiProgress {
    inner: IndicatifMultiProgress,
}

impl MultiProgress {
    pub fn new() -> Self {
        Self {
            inner: IndicatifMultiProgress::new(),
        }
    }

    /// Wrap an existing `indicatif::MultiProgress`.
    pub fn from_raw(mp: IndicatifMultiProgress) -> Self {
        Self { inner: mp }
    }

    /// Access the underlying `indicatif::MultiProgress`.
    pub fn raw(&self) -> &IndicatifMultiProgress {
        &self.inner
    }

    /// Add a bar-style progress with the given prefix and total.
    pub fn add_bar(&self, prefix: impl Into<String>, total: u64) -> ProgressBar {
        let bar = IndicatifBar::new(total);
        bar.set_style(ProgressStyle::Bar.to_indicatif());
        bar.set_prefix(prefix.into());
        let bar = self.inner.add(bar);
        ProgressBar { inner: bar }
    }

    /// Add a bar inserted after another bar (keeps ordering stable).
    pub fn insert_bar_after(
        &self,
        after: &ProgressBar,
        prefix: impl Into<String>,
        total: u64,
    ) -> ProgressBar {
        let bar = IndicatifBar::new(total);
        bar.set_style(ProgressStyle::Bar.to_indicatif());
        bar.set_prefix(prefix.into());
        let bar = self.inner.insert_after(after.inner(), bar);
        ProgressBar { inner: bar }
    }

    /// Add a spinner-style progress.
    pub fn add_spinner(&self, prefix: impl Into<String>) -> ProgressBar {
        let bar = IndicatifBar::new_spinner();
        bar.set_style(ProgressStyle::Spinner.to_indicatif());
        bar.set_prefix(prefix.into());
        bar.enable_steady_tick(Duration::from_millis(120));
        let bar = self.inner.add(bar);
        ProgressBar { inner: bar }
    }

    /// Add a static text line (no animation, no bar).
    pub fn add_static_line(
        &self,
        prefix: impl Into<String>,
        msg: impl Into<std::borrow::Cow<'static, str>>,
    ) -> ProgressBar {
        let bar = IndicatifBar::new(0);
        bar.set_style(ProgressStyle::Finished.to_indicatif());
        bar.set_prefix(prefix.into());
        bar.finish_with_message(msg);
        let bar = self.inner.add(bar);
        ProgressBar { inner: bar }
    }

    /// Print a line above the progress bars (for completed items).
    pub fn println(&self, msg: impl AsRef<str>) -> std::io::Result<()> {
        self.inner.println(msg)
    }

    /// Remove a progress bar from the display.
    pub fn remove(&self, bar: &ProgressBar) {
        self.inner.remove(bar.inner());
    }

    /// Clear all bars.
    pub fn clear(&self) -> std::io::Result<()> {
        self.inner.clear()
    }
}

impl Default for MultiProgress {
    fn default() -> Self {
        Self::new()
    }
}
