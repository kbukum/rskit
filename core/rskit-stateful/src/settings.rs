//! Serde-loadable accumulator settings.
//!
//! [`AccumulatorSettings`] describes an accumulator's TTL, capacity, and flush triggers in a
//! form that can be loaded from configuration without serializing trait objects. Calling
//! [`AccumulatorSettings::build`] instantiates the concrete, still-pluggable [`Trigger`] values
//! into an [`AccumulatorConfig`], so custom triggers and measurers remain available in code.

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::config::AccumulatorConfig;
use crate::trigger::{ByteSizeTrigger, SizeTrigger, TimeTrigger, Trigger};

/// Declarative flush-trigger specification loadable from configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum TriggerSpec {
    /// Flush when the buffered item count reaches `threshold`.
    Size {
        /// Item-count threshold that triggers a flush.
        threshold: usize,
    },
    /// Flush when the measured byte size reaches `threshold`.
    ByteSize {
        /// Byte-size threshold that triggers a flush.
        threshold: usize,
    },
    /// Flush when time since the last flush reaches `interval`.
    Time {
        /// Interval after which a flush is triggered.
        #[serde(with = "rskit_util::time::serde_duration")]
        interval: Duration,
    },
}

impl TriggerSpec {
    /// Build the concrete pluggable [`Trigger`] described by this spec.
    #[must_use]
    pub fn build<V>(&self) -> Arc<dyn Trigger<V>>
    where
        V: Clone + Send + Sync + 'static,
    {
        match *self {
            Self::Size { threshold } => Arc::new(SizeTrigger::new(threshold)),
            Self::ByteSize { threshold } => Arc::new(ByteSizeTrigger::new(threshold)),
            Self::Time { interval } => Arc::new(TimeTrigger::new(interval)),
        }
    }
}

/// Loadable accumulator configuration describing TTL, capacity, and flush triggers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AccumulatorSettings {
    /// Optional expiration TTL.
    #[serde(
        with = "rskit_util::time::serde_duration::option",
        skip_serializing_if = "Option::is_none"
    )]
    pub ttl: Option<Duration>,
    /// Whether TTL should be refreshed on append/touch.
    pub keep_alive: bool,
    /// Optional maximum measured size before oldest values are evicted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_size: Option<usize>,
    /// Flush triggers evaluated after each append.
    pub triggers: Vec<TriggerSpec>,
}

impl Default for AccumulatorSettings {
    fn default() -> Self {
        Self {
            ttl: None,
            keep_alive: true,
            max_size: None,
            triggers: Vec::new(),
        }
    }
}

impl AccumulatorSettings {
    /// Build an [`AccumulatorConfig`] with the configured TTL, capacity, and triggers.
    ///
    /// The default [`CountMeasurer`](crate::measurer::CountMeasurer) is used; override it with
    /// [`AccumulatorConfig::with_measurer`] on the returned config when a different measurer is
    /// required.
    #[must_use]
    pub fn build<V>(&self) -> AccumulatorConfig<V>
    where
        V: Clone + Send + Sync + 'static,
    {
        let mut config = AccumulatorConfig::new();
        if let Some(ttl) = self.ttl {
            config = config.with_ttl(ttl);
        }
        if !self.keep_alive {
            config = config.without_keep_alive();
        }
        if let Some(max_size) = self.max_size {
            config = config.with_max_size(max_size);
        }
        for spec in &self.triggers {
            config = config.with_trigger(spec.build());
        }
        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_build_triggers() {
        let settings = AccumulatorSettings {
            ttl: Some(Duration::from_secs(30)),
            keep_alive: false,
            max_size: Some(100),
            triggers: vec![
                TriggerSpec::Size { threshold: 10 },
                TriggerSpec::ByteSize { threshold: 4096 },
                TriggerSpec::Time {
                    interval: Duration::from_secs(5),
                },
            ],
        };

        let json = serde_json::to_string(&settings).unwrap();
        let restored: AccumulatorSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, settings);

        let config = settings.build::<Vec<u8>>();
        assert_eq!(config.ttl, Some(Duration::from_secs(30)));
        assert!(!config.keep_alive);
        assert_eq!(config.max_size, Some(100));
        assert_eq!(config.triggers.len(), 3);
    }

    #[test]
    fn defaults_omit_ttl_and_keep_alive_is_true() {
        let settings: AccumulatorSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings, AccumulatorSettings::default());
        assert!(settings.keep_alive);
        assert!(settings.ttl.is_none());

        let json = serde_json::to_string(&settings).unwrap();
        assert!(!json.contains("ttl"));
        assert!(!json.contains("max_size"));
    }

    #[test]
    fn ttl_and_interval_round_trip_non_round_durations_losslessly() {
        let settings = AccumulatorSettings {
            ttl: Some(Duration::from_secs(3601)),
            keep_alive: true,
            max_size: None,
            triggers: vec![TriggerSpec::Time {
                interval: Duration::new(90, 500),
            }],
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("\"3601s\""), "ttl not lossless: {json}");
        let restored: AccumulatorSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, settings);
    }

    #[test]
    fn trigger_spec_uses_tagged_snake_case() {
        let json = serde_json::to_string(&TriggerSpec::ByteSize { threshold: 8 }).unwrap();
        assert_eq!(json, r#"{"type":"byte_size","threshold":8}"#);
    }
}
