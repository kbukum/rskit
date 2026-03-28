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

    /// Parse a single FFmpeg stderr line. Returns `Some(Progress)` if the line
    /// contains progress information.
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
        let speed = extract_value(line, "speed=").and_then(|v| {
            v.trim_end_matches('x').parse::<f64>().ok()
        });
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
