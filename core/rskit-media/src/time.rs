//! Time-related types for media processing.

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// A time point in microseconds from the start of the media.
///
/// Microsecond precision matches FFmpeg's internal timestamp resolution,
/// avoiding precision loss during conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Timestamp(pub u64);

impl Timestamp {
    /// Create from milliseconds.
    pub fn from_millis(ms: u64) -> Self {
        Self(ms.saturating_mul(1000))
    }

    /// Create from microseconds.
    pub fn from_micros(us: u64) -> Self {
        Self(us)
    }

    /// Create from seconds (floating point).
    pub fn from_seconds(s: f64) -> Self {
        Self((s * 1_000_000.0) as u64)
    }

    /// Create from hours, minutes, and seconds.
    pub fn from_hms(h: u32, m: u32, s: f64) -> Self {
        let total_secs = (h as f64) * 3600.0 + (m as f64) * 60.0 + s;
        Self::from_seconds(total_secs)
    }

    /// Get the value in milliseconds (truncates sub-millisecond part).
    pub fn as_millis(&self) -> u64 {
        self.0 / 1000
    }

    /// Get the value in microseconds.
    pub fn as_micros(&self) -> u64 {
        self.0
    }

    /// Get the value in seconds (floating point).
    pub fn as_seconds(&self) -> f64 {
        self.0 as f64 / 1_000_000.0
    }

    /// Convert to a [`Duration`].
    pub fn as_duration(&self) -> Duration {
        Duration::from_micros(self.0)
    }

    /// Format as "HH:MM:SS.mmm" (FFmpeg-compatible).
    pub fn to_ffmpeg_time(&self) -> String {
        let total_us = self.0;
        let ms = (total_us / 1000) % 1000;
        let total_secs = total_us / 1_000_000;
        let secs = total_secs % 60;
        let total_mins = total_secs / 60;
        let mins = total_mins % 60;
        let hours = total_mins / 60;
        format!("{hours:02}:{mins:02}:{secs:02}.{ms:03}")
    }
}

impl std::fmt::Display for Timestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_ffmpeg_time())
    }
}

/// A time range within a media file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeRange {
    /// Start of the range.
    pub start: Timestamp,
    /// End of the range.
    pub end: Timestamp,
}

impl TimeRange {
    /// Create a new time range.
    pub fn new(start: Timestamp, end: Timestamp) -> Self {
        Self { start, end }
    }

    /// Create from millisecond values.
    pub fn from_millis(start_ms: u64, end_ms: u64) -> Self {
        Self {
            start: Timestamp::from_millis(start_ms),
            end: Timestamp::from_millis(end_ms),
        }
    }

    /// Create from second values.
    pub fn from_seconds(start: f64, end: f64) -> Self {
        Self {
            start: Timestamp::from_seconds(start),
            end: Timestamp::from_seconds(end),
        }
    }

    /// Duration of this range.
    pub fn duration(&self) -> Duration {
        Duration::from_micros(self.end.0.saturating_sub(self.start.0))
    }

    /// Duration of this range in milliseconds.
    pub fn duration_ms(&self) -> u64 {
        self.end.as_millis().saturating_sub(self.start.as_millis())
    }

    /// Check if a timestamp falls within this range.
    pub fn contains(&self, ts: Timestamp) -> bool {
        ts >= self.start && ts <= self.end
    }

    /// Check if this range overlaps with another.
    pub fn overlaps(&self, other: &TimeRange) -> bool {
        self.start < other.end && other.start < self.end
    }

    /// Merge two overlapping ranges into one, if they overlap.
    pub fn merge(&self, other: &TimeRange) -> Option<TimeRange> {
        if self.overlaps(other) {
            Some(TimeRange {
                start: std::cmp::min(self.start, other.start),
                end: std::cmp::max(self.end, other.end),
            })
        } else {
            None
        }
    }

    /// Split this range at a timestamp.
    pub fn split_at(&self, ts: Timestamp) -> (Option<TimeRange>, Option<TimeRange>) {
        if ts <= self.start {
            (None, Some(*self))
        } else if ts >= self.end {
            (Some(*self), None)
        } else {
            (
                Some(TimeRange::new(self.start, ts)),
                Some(TimeRange::new(ts, self.end)),
            )
        }
    }

    /// Shift this range by a signed millisecond offset.
    pub fn shift(&self, offset_ms: i64) -> Self {
        let offset_us = offset_ms * 1000;
        let shift = |ts: Timestamp| {
            if offset_us >= 0 {
                Timestamp(ts.0.saturating_add(offset_us as u64))
            } else {
                Timestamp(ts.0.saturating_sub(offset_us.unsigned_abs()))
            }
        };
        Self {
            start: shift(self.start),
            end: shift(self.end),
        }
    }
}

/// A labeled segment within a media file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    /// Time range of this segment.
    pub range: TimeRange,
    /// Optional label (e.g., "intro", "chorus").
    pub label: Option<String>,
    /// Optional confidence score (0.0 – 1.0).
    pub confidence: Option<f32>,
    /// Arbitrary metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Segment {
    /// Create a new segment with the given time range.
    pub fn new(range: TimeRange) -> Self {
        Self {
            range,
            label: None,
            confidence: None,
            metadata: HashMap::new(),
        }
    }

    /// Set the label.
    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set the confidence score (clamped to 0.0–1.0).
    #[must_use]
    pub fn with_confidence(mut self, c: f32) -> Self {
        self.confidence = Some(c.clamp(0.0, 1.0));
        self
    }

    /// Add a metadata key-value pair.
    #[must_use]
    pub fn with_meta(mut self, key: impl Into<String>, val: impl Into<serde_json::Value>) -> Self {
        self.metadata.insert(key.into(), val.into());
        self
    }
}
