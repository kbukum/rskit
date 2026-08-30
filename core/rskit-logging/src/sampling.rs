//! Rate-based log sampling layer.
//!
//! Limits log throughput per level to prevent log storms in production.
//! After an initial burst of messages the layer drops a configurable fraction of events,
//! keeping resource usage predictable.
//!
//! # Example
//!
//! ```rust
//! use rskit_logging::sampling::SamplingConfig;
//!
//! let cfg = SamplingConfig { enabled: true, initial_rate: 50, thereafter_rate: 10 };
//! assert!(cfg.enabled);
//! ```

use std::time::Instant;

use parking_lot::Mutex;
use std::collections::HashMap;
use tracing::level_filters::LevelFilter;
use tracing::{Level, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

// ── Config ──────────────────────────────────────────────────────────────────

pub use crate::config::SamplingConfig;

// ── Per-level counter ───────────────────────────────────────────────────────

/// Tracks event count within a one-second window.
#[derive(Debug)]
struct LevelCounter {
    window_start: Instant,
    count: u32,
}

impl LevelCounter {
    fn new() -> Self {
        Self {
            window_start: Instant::now(),
            count: 0,
        }
    }
}

// ── Layer ───────────────────────────────────────────────────────────────────

/// A [`tracing_subscriber::Layer`] that drops events exceeding per-level rate limits.
///
/// Events within the initial burst (`initial_rate` per second per level) are always passed through.
/// After the burst, only every `thereafter_rate`-th event is kept.
pub struct SamplingLayer {
    initial_rate: u32,
    thereafter_rate: u32,
    enabled: bool,
    counters: Mutex<HashMap<Level, LevelCounter>>,
}

impl SamplingLayer {
    /// Create a new sampling layer from a [`SamplingConfig`].
    pub fn new(cfg: &SamplingConfig) -> Self {
        Self {
            initial_rate: cfg.initial_rate,
            thereafter_rate: cfg.thereafter_rate,
            enabled: cfg.enabled,
            counters: Mutex::new(HashMap::new()),
        }
    }

    /// Determine whether an event at the given level should be kept.
    fn should_keep(&self, level: Level) -> bool {
        // The master switch is authoritative even for direct users of this public layer: when
        // sampling is disabled every event passes through.
        if !self.enabled {
            return true;
        }

        let mut counters = self.counters.lock();
        let counter = counters.entry(level).or_insert_with(LevelCounter::new);

        let elapsed = counter.window_start.elapsed();
        if elapsed.as_secs() >= 1 {
            // Reset the window.
            counter.window_start = Instant::now();
            counter.count = 1;
            return true;
        }

        counter.count += 1;

        if counter.count <= self.initial_rate {
            return true;
        }

        // After the burst: keep every Nth event. A `thereafter_rate` of 0 drops everything
        // after the burst; a rate of 1 keeps all (remainder is always 0).
        if self.thereafter_rate == 0 {
            return false;
        }
        let excess = counter.count - self.initial_rate;
        excess.is_multiple_of(self.thereafter_rate)
    }
}

impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for SamplingLayer {
    fn event_enabled(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) -> bool {
        self.should_keep(*event.metadata().level())
    }

    fn max_level_hint(&self) -> Option<LevelFilter> {
        // We do not restrict any levels — all are eligible for sampling.
        None
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_disabled() {
        let cfg = SamplingConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.initial_rate, 100);
        assert_eq!(cfg.thereafter_rate, 100);
    }

    #[test]
    fn allows_initial_burst() {
        let layer = SamplingLayer::new(&SamplingConfig {
            enabled: true,
            initial_rate: 5,
            thereafter_rate: 2,
        });

        for _ in 0..5 {
            assert!(layer.should_keep(Level::INFO));
        }
    }

    #[test]
    fn drops_after_burst() {
        let layer = SamplingLayer::new(&SamplingConfig {
            enabled: true,
            initial_rate: 2,
            thereafter_rate: 3,
        });

        // Burst
        assert!(layer.should_keep(Level::INFO));
        assert!(layer.should_keep(Level::INFO));

        // After burst: keep every 3rd (excess % 3 == 0)
        assert!(!layer.should_keep(Level::INFO)); // excess=1 → 1%3==1 ✗
        assert!(!layer.should_keep(Level::INFO)); // excess=2 → 2%3==2 ✗
        assert!(layer.should_keep(Level::INFO)); // excess=3 → 3%3==0 ✓
        assert!(!layer.should_keep(Level::INFO)); // excess=4 → 4%3==1 ✗
    }

    #[test]
    fn thereafter_rate_one_keeps_all() {
        let layer = SamplingLayer::new(&SamplingConfig {
            enabled: true,
            initial_rate: 1,
            thereafter_rate: 1,
        });

        // Every event after the burst is kept (remainder is always 0).
        for _ in 0..10 {
            assert!(layer.should_keep(Level::INFO));
        }
    }

    #[test]
    fn per_level_independent_counting() {
        let layer = SamplingLayer::new(&SamplingConfig {
            enabled: true,
            initial_rate: 1,
            thereafter_rate: 2,
        });

        // Each level has its own counter.
        assert!(layer.should_keep(Level::INFO));
        assert!(layer.should_keep(Level::WARN));

        // Second event per level — exceeds burst. excess=1 → 1%2==1 ✗
        assert!(!layer.should_keep(Level::INFO));
        assert!(!layer.should_keep(Level::WARN));

        // Third event — excess=2 → 2%2==0 ✓
        assert!(layer.should_keep(Level::INFO));
        assert!(layer.should_keep(Level::WARN));
    }

    #[test]
    fn thereafter_rate_zero_drops_after_burst() {
        let layer = SamplingLayer::new(&SamplingConfig {
            enabled: true,
            initial_rate: 1,
            thereafter_rate: 0,
        });

        // The initial burst passes, then everything after is dropped.
        assert!(layer.should_keep(Level::ERROR));
        assert!(!layer.should_keep(Level::ERROR));
        assert!(!layer.should_keep(Level::ERROR));
    }

    #[test]
    fn disabled_layer_keeps_every_event() {
        let layer = SamplingLayer::new(&SamplingConfig {
            enabled: false,
            initial_rate: 1,
            thereafter_rate: 2,
        });

        // The master switch is off, so nothing is sampled even past the burst.
        for _ in 0..20 {
            assert!(layer.should_keep(Level::INFO));
        }
    }
}
