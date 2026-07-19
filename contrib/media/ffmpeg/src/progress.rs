//! FFmpeg progress output parser.

use std::time::Duration;

use rskit_media::{pipeline::Progress, time::Timestamp};

/// Parses FFmpeg stderr progress lines into [`Progress`] reports.
pub(crate) struct FfmpegProgressParser {
    total_duration: Option<Duration>,
}

impl FfmpegProgressParser {
    pub fn new(total_duration: Option<Duration>) -> Self {
        Self { total_duration }
    }

    /// Parse a single FFmpeg stderr line.
    /// Returns `Some(Progress)` if the line contains progress information.
    ///
    /// FFmpeg outputs lines like:
    /// ```text
    /// frame= 1234 fps=120 q=23.0 size=   12345kB time=00:01:23.45 bitrate=1234.5kbits/s speed=2.5x
    /// ```
    pub fn parse_line(&self, line: &str) -> Option<Progress> {
        if !line.contains("time=") {
            return None;
        }

        let time_ms = extract_value(line, "time=").and_then(|v| parse_ffmpeg_time(&v));
        let speed =
            extract_value(line, "speed=").and_then(|v| v.trim_end_matches('x').parse::<f64>().ok());
        let size_kb = extract_value(line, "size=").and_then(|v| {
            let v = v.trim().trim_end_matches("kB").trim();
            v.parse::<u64>().ok()
        });

        let position = time_ms.map(Timestamp::from_millis);
        let percent = match (time_ms, self.total_duration) {
            (Some(current), Some(total)) if total.as_millis() > 0 => {
                Some((current as f64 / total.as_millis() as f64 * 100.0) as f32)
            }
            _ => None,
        };

        let eta = match (percent, speed) {
            (Some(pct), Some(spd)) if spd > 0.0 && pct > 0.0 && pct < 100.0 => {
                let _remaining_pct = 100.0 - pct;
                let elapsed_ms = time_ms.unwrap_or(0) as f64;
                if elapsed_ms > 0.0 {
                    let total_estimated = elapsed_ms / (pct as f64 / 100.0);
                    let remaining = total_estimated - elapsed_ms;
                    Some(Duration::from_millis((remaining / spd) as u64))
                } else {
                    None
                }
            }
            _ => None,
        };

        Some(Progress {
            position,
            total: self.total_duration,
            percent,
            speed,
            output_size: size_kb.map(|kb| kb * 1024),
            eta,
        })
    }
}

fn extract_value(line: &str, key: &str) -> Option<String> {
    let start = line.find(key)? + key.len();
    let rest = &line[start..];
    let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

fn parse_ffmpeg_time(s: &str) -> Option<u64> {
    // Format: HH:MM:SS.ss or HH:MM:SS.sss
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 3 {
        return None;
    }
    let h: u64 = parts[0].parse().ok()?;
    let m: u64 = parts[1].parse().ok()?;
    let (sec, frac) = if let Some((s, f)) = parts[2].split_once('.') {
        let sec: u64 = s.parse().ok()?;
        let frac_str = format!("{:0<3}", &f[..f.len().min(3)]);
        let frac: u64 = frac_str.parse().ok()?;
        (sec, frac)
    } else {
        (parts[2].parse().ok()?, 0)
    };

    Some(h * 3_600_000 + m * 60_000 + sec * 1000 + frac)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_progress_line_with_time_and_speed() {
        let parser = FfmpegProgressParser::new(Some(Duration::from_secs(60)));
        let line = "frame= 1234 fps=120.0 q=23.0 size=   12345kB time=00:00:30.00 bitrate=1234.5kbits/s speed=2.00x";
        let progress = parser.parse_line(line).expect("should parse");
        assert_eq!(progress.position.unwrap().as_millis(), 30000);
        assert!((progress.speed.unwrap() - 2.0).abs() < f64::EPSILON);
        assert!(progress.percent.is_some());
        let pct = progress.percent.unwrap();
        assert!((pct - 50.0).abs() < 1.0, "expected ~50%, got {pct}");
    }

    #[test]
    fn parse_progress_line_no_speed() {
        let parser = FfmpegProgressParser::new(None);
        let line =
            "frame=  100 fps=30.0 size=   500kB time=00:01:00.50 bitrate=100kbits/s speed=N/A";
        let progress = parser.parse_line(line).expect("should parse");
        assert_eq!(progress.position.unwrap().as_millis(), 60500);
        assert!(progress.speed.is_none()); // N/A should fail parse
        assert!(progress.percent.is_none()); // no total_duration
    }

    #[test]
    fn parse_non_progress_line_returns_none() {
        let parser = FfmpegProgressParser::new(None);
        assert!(parser.parse_line("  Duration: 00:01:30.00").is_none());
        assert!(parser.parse_line("Stream #0:0: Video: h264").is_none());
        assert!(parser.parse_line("").is_none());
    }

    #[test]
    fn parse_ffmpeg_time_hms() {
        assert_eq!(parse_ffmpeg_time("00:00:00.000"), Some(0));
        assert_eq!(parse_ffmpeg_time("00:01:00.000"), Some(60000));
        assert_eq!(parse_ffmpeg_time("01:30:15.500"), Some(5415500));
        assert_eq!(parse_ffmpeg_time("00:00:01.50"), Some(1500));
    }

    #[test]
    fn parse_ffmpeg_time_invalid() {
        assert!(parse_ffmpeg_time("invalid").is_none());
        assert!(parse_ffmpeg_time("00:00").is_none());
    }

    #[test]
    fn extract_value_from_line() {
        let line = "frame=100 fps=30 time=00:01:00.00 speed=2x";
        assert_eq!(extract_value(line, "time="), Some("00:01:00.00".into()));
        assert_eq!(extract_value(line, "speed="), Some("2x".into()));
        assert_eq!(extract_value(line, "missing="), None);
    }

    #[test]
    fn progress_with_output_size() {
        let parser = FfmpegProgressParser::new(None);
        // Real ffmpeg format has no space after size=
        let line = "frame=50 fps=25 size=2048kB time=00:00:10.00 bitrate=100kbits/s speed=1x";
        let progress = parser.parse_line(line).unwrap();
        assert_eq!(progress.output_size, Some(2048 * 1024));
    }
}
