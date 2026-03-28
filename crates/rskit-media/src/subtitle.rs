//! Subtitle types and SRT/VTT parsing.

use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::{Deserialize, Serialize};

use crate::time::TimeRange;

/// A single subtitle entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleEntry {
    /// Time range when this subtitle is displayed.
    pub range: TimeRange,
    /// The subtitle text.
    pub text: String,
    /// Optional styling for this entry.
    pub style: Option<SubtitleStyle>,
}

/// Subtitle visual style.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleStyle {
    /// Font family name.
    pub font_family: Option<String>,
    /// Font size in points.
    pub font_size: Option<u16>,
    /// Text color (CSS format, e.g., "#FFFFFF").
    pub color: Option<String>,
    /// Background color.
    pub background: Option<String>,
    /// Whether text is bold.
    pub bold: bool,
    /// Whether text is italic.
    pub italic: bool,
    /// Where to position the subtitle.
    pub position: SubtitlePosition,
}

/// Subtitle display position.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SubtitlePosition {
    /// Bottom of the screen (default).
    Bottom,
    /// Top of the screen.
    Top,
    /// Center of the screen.
    Center,
    /// Custom pixel coordinates.
    Custom {
        /// X coordinate.
        x: u32,
        /// Y coordinate.
        y: u32,
    },
}

impl Default for SubtitlePosition {
    fn default() -> Self {
        Self::Bottom
    }
}

/// A collection of subtitle entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleTrack {
    /// Subtitle entries sorted by time.
    pub entries: Vec<SubtitleEntry>,
    /// Track language (BCP 47 tag).
    pub language: Option<String>,
    /// Default style for all entries (can be overridden per entry).
    pub default_style: Option<SubtitleStyle>,
}

impl SubtitleTrack {
    /// Create an empty subtitle track.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            language: None,
            default_style: None,
        }
    }

    /// Add a subtitle entry (builder pattern).
    #[must_use]
    pub fn add(mut self, range: TimeRange, text: impl Into<String>) -> Self {
        self.entries.push(SubtitleEntry {
            range,
            text: text.into(),
            style: None,
        });
        self
    }

    /// Set the track language.
    #[must_use]
    pub fn with_language(mut self, lang: impl Into<String>) -> Self {
        self.language = Some(lang.into());
        self
    }

    /// Parse SRT format subtitle content.
    pub fn from_srt(content: &str) -> AppResult<Self> {
        let mut entries = Vec::new();
        let blocks: Vec<&str> = content.split("\n\n").collect();

        for block in blocks {
            let lines: Vec<&str> = block.trim().lines().collect();
            if lines.len() < 3 {
                continue;
            }

            // Line 0: index (ignored)
            // Line 1: timestamps "HH:MM:SS,mmm --> HH:MM:SS,mmm"
            let time_line = lines[1];
            let parts: Vec<&str> = time_line.split(" --> ").collect();
            if parts.len() != 2 {
                continue;
            }

            let start = parse_srt_time(parts[0].trim()).ok_or_else(|| {
                AppError::new(ErrorCode::InvalidFormat, format!("invalid SRT time: {}", parts[0]))
            })?;
            let end = parse_srt_time(parts[1].trim()).ok_or_else(|| {
                AppError::new(ErrorCode::InvalidFormat, format!("invalid SRT time: {}", parts[1]))
            })?;

            let text = lines[2..].join("\n");

            entries.push(SubtitleEntry {
                range: TimeRange::from_millis(start, end),
                text,
                style: None,
            });
        }

        Ok(Self {
            entries,
            language: None,
            default_style: None,
        })
    }

    /// Parse WebVTT format subtitle content.
    pub fn from_vtt(content: &str) -> AppResult<Self> {
        let content = content.strip_prefix("WEBVTT").unwrap_or(content);
        let mut entries = Vec::new();
        let blocks: Vec<&str> = content.split("\n\n").collect();

        for block in blocks {
            let lines: Vec<&str> = block.trim().lines().collect();
            if lines.is_empty() {
                continue;
            }

            // Find the timestamp line
            let time_idx = lines.iter().position(|l| l.contains(" --> "));
            let Some(time_idx) = time_idx else {
                continue;
            };

            let time_line = lines[time_idx];
            let parts: Vec<&str> = time_line.split(" --> ").collect();
            if parts.len() != 2 {
                continue;
            }

            let start_str = parts[0].trim();
            let end_str = parts[1].trim().split_whitespace().next().unwrap_or("");

            let start = parse_vtt_time(start_str).ok_or_else(|| {
                AppError::new(ErrorCode::InvalidFormat, format!("invalid VTT time: {start_str}"))
            })?;
            let end = parse_vtt_time(end_str).ok_or_else(|| {
                AppError::new(ErrorCode::InvalidFormat, format!("invalid VTT time: {end_str}"))
            })?;

            let text = lines[time_idx + 1..].join("\n");
            if text.is_empty() {
                continue;
            }

            entries.push(SubtitleEntry {
                range: TimeRange::from_millis(start, end),
                text,
                style: None,
            });
        }

        Ok(Self {
            entries,
            language: None,
            default_style: None,
        })
    }

    /// Serialize to SRT format.
    pub fn to_srt(&self) -> String {
        let mut out = String::new();
        for (i, entry) in self.entries.iter().enumerate() {
            out.push_str(&format!("{}\n", i + 1));
            out.push_str(&format!(
                "{} --> {}\n",
                format_srt_time(entry.range.start.as_millis()),
                format_srt_time(entry.range.end.as_millis()),
            ));
            out.push_str(&entry.text);
            out.push_str("\n\n");
        }
        out
    }

    /// Serialize to WebVTT format.
    pub fn to_vtt(&self) -> String {
        let mut out = String::from("WEBVTT\n\n");
        for entry in &self.entries {
            out.push_str(&format!(
                "{} --> {}\n",
                format_vtt_time(entry.range.start.as_millis()),
                format_vtt_time(entry.range.end.as_millis()),
            ));
            out.push_str(&entry.text);
            out.push_str("\n\n");
        }
        out
    }

    /// Shift all entries by the given millisecond offset.
    pub fn shift(&mut self, offset: i64) {
        for entry in &mut self.entries {
            entry.range = entry.range.shift(offset);
        }
    }

    /// Return a new track containing only entries that overlap the given range.
    pub fn in_range(&self, range: &TimeRange) -> Self {
        Self {
            entries: self
                .entries
                .iter()
                .filter(|e| e.range.overlaps(range))
                .cloned()
                .collect(),
            language: self.language.clone(),
            default_style: self.default_style.clone(),
        }
    }
}

impl Default for SubtitleTrack {
    fn default() -> Self {
        Self::new()
    }
}

// ── SRT time parsing: "HH:MM:SS,mmm" ────────────────────────────────

fn parse_srt_time(s: &str) -> Option<u64> {
    let s = s.replace(',', ".");
    parse_time_dotted(&s)
}

fn format_srt_time(ms: u64) -> String {
    let millis = ms % 1000;
    let total_secs = ms / 1000;
    let secs = total_secs % 60;
    let total_mins = total_secs / 60;
    let mins = total_mins % 60;
    let hours = total_mins / 60;
    format!("{hours:02}:{mins:02}:{secs:02},{millis:03}")
}

// ── VTT time parsing: "HH:MM:SS.mmm" or "MM:SS.mmm" ─────────────────

fn parse_vtt_time(s: &str) -> Option<u64> {
    parse_time_dotted(s)
}

fn format_vtt_time(ms: u64) -> String {
    let millis = ms % 1000;
    let total_secs = ms / 1000;
    let secs = total_secs % 60;
    let total_mins = total_secs / 60;
    let mins = total_mins % 60;
    let hours = total_mins / 60;
    format!("{hours:02}:{mins:02}:{secs:02}.{millis:03}")
}

fn parse_time_dotted(s: &str) -> Option<u64> {
    let (main, frac) = if let Some((m, f)) = s.split_once('.') {
        (m, f.parse::<u64>().ok()?)
    } else {
        (s, 0)
    };

    let parts: Vec<&str> = main.split(':').collect();
    let (h, m, sec) = match parts.len() {
        3 => (
            parts[0].parse::<u64>().ok()?,
            parts[1].parse::<u64>().ok()?,
            parts[2].parse::<u64>().ok()?,
        ),
        2 => (
            0,
            parts[0].parse::<u64>().ok()?,
            parts[1].parse::<u64>().ok()?,
        ),
        _ => return None,
    };

    Some(h * 3_600_000 + m * 60_000 + sec * 1000 + frac)
}
