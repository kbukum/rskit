//! Detection and structural analysis — scenes, keyframes, silence, chapters.

use std::time::Duration;

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_file::FileSource;
use rskit_media::{
    probe::{Chapter, KeyframeInfo, PictureType, SilenceInterval},
    time::{TimeRange, Timestamp},
};

use super::FfmpegProbe;

impl FfmpegProbe {
    /// Detect scene changes via FFmpeg's `select` filter with `scene` metric.
    pub(crate) async fn detect_scenes(
        &self,
        source: &FileSource,
        threshold: f64,
    ) -> AppResult<Vec<Timestamp>> {
        let resolved = source.to_local_path().await?;
        let threshold = threshold.clamp(0.0, 1.0);

        let output = tokio::process::Command::new(self.config.ffmpeg_bin())
            .args([
                "-i",
                &resolved.path().to_string_lossy(),
                "-vf",
                &format!("select='gt(scene\\,{threshold})',showinfo"),
                "-f",
                "null",
                "-",
            ])
            .output()
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("ffmpeg scene_detect failed: {e}"),
                )
            })?;

        let stderr = String::from_utf8_lossy(&output.stderr);
        Ok(parse_showinfo_timestamps(&stderr))
    }

    /// Extract keyframe positions via `ffprobe -show_frames`.
    ///
    /// Runs ffprobe with frame-level output on the video stream to find
    /// all I-frames / IDR frames. Only keyframes are included in the result.
    pub(crate) async fn extract_keyframes(
        &self,
        source: &FileSource,
    ) -> AppResult<Vec<KeyframeInfo>> {
        let resolved = source.to_local_path().await?;

        let output = tokio::process::Command::new(self.config.ffprobe_bin())
            .args([
                "-v",
                "quiet",
                "-select_streams",
                "v:0",
                "-show_frames",
                "-show_entries",
                "frame=pts_time,pkt_size,pict_type,key_frame,coded_picture_number",
                "-print_format",
                "json",
            ])
            .arg(resolved.path())
            .output()
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("ffprobe keyframes failed: {e}"),
                )
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::new(
                ErrorCode::Internal,
                format!("ffprobe keyframes failed: {stderr}"),
            ));
        }

        let json: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("ffprobe keyframes output is not valid JSON: {e}"),
            )
        })?;

        Ok(parse_keyframes(&json))
    }

    /// Detect silence intervals via FFmpeg's `silencedetect` audio filter.
    pub(crate) async fn detect_silence(
        &self,
        source: &FileSource,
        min_duration: Duration,
        noise_threshold_db: f64,
    ) -> AppResult<Vec<SilenceInterval>> {
        let resolved = source.to_local_path().await?;
        let threshold_db = noise_threshold_db.clamp(-96.0, 0.0);
        let min_secs = min_duration.as_secs_f64().max(0.01);

        let output = tokio::process::Command::new(self.config.ffmpeg_bin())
            .args([
                "-i",
                &resolved.path().to_string_lossy(),
                "-af",
                &format!("silencedetect=noise={threshold_db}dB:d={min_secs}"),
                "-f",
                "null",
                "-",
            ])
            .output()
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("ffmpeg silence_detect failed: {e}"),
                )
            })?;

        let stderr = String::from_utf8_lossy(&output.stderr);
        Ok(parse_silence_intervals(&stderr))
    }

    /// Extract chapter markers from the media container via ffprobe.
    pub(crate) async fn extract_chapters(&self, source: &FileSource) -> AppResult<Vec<Chapter>> {
        let json = self.probe_raw(source).await?;
        Ok(parse_chapters(&json))
    }
}

// ── Parsers ─────────────────────────────────────────────────────────────────

/// Parse `pts_time` values from FFmpeg showinfo output lines.
fn parse_showinfo_timestamps(stderr: &str) -> Vec<Timestamp> {
    let mut timestamps = Vec::new();
    for line in stderr.lines() {
        if let Some(pts_idx) = line.find("pts_time:") {
            let after = &line[pts_idx + 9..];
            let end = after
                .find(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
                .unwrap_or(after.len());
            if let Ok(secs) = after[..end].trim().parse::<f64>() {
                timestamps.push(Timestamp::from_millis((secs * 1000.0).round() as u64));
            }
        }
    }
    timestamps
}

/// Parse ffprobe `-show_frames` JSON into keyframe info.
fn parse_keyframes(json: &serde_json::Value) -> Vec<KeyframeInfo> {
    let frames = match json.get("frames").and_then(|f| f.as_array()) {
        Some(f) => f,
        None => return Vec::new(),
    };

    let mut keyframes = Vec::new();
    let mut frame_number: u64 = 0;

    for frame in frames {
        let is_key = frame.get("key_frame").and_then(|v| v.as_u64()).unwrap_or(0) == 1;

        if is_key {
            let pts_time = frame
                .get("pts_time")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0);

            let pict_type = frame
                .get("pict_type")
                .and_then(|v| v.as_str())
                .map(PictureType::from_ffprobe)
                .unwrap_or(PictureType::I);

            let size_bytes = frame
                .get("pkt_size")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<u64>().ok())
                .or_else(|| frame.get("pkt_size").and_then(|v| v.as_u64()));

            keyframes.push(KeyframeInfo {
                timestamp: Timestamp::from_seconds(pts_time),
                frame_number,
                picture_type: pict_type,
                size_bytes,
            });
        }

        frame_number += 1;
    }

    keyframes
}

/// Parse FFmpeg silencedetect output.
///
/// The filter outputs pairs of lines:
/// ```text
/// [silencedetect @ 0x...] silence_start: 1.234
/// [silencedetect @ 0x...] silence_end: 5.678 | silence_duration: 4.444
/// ```
fn parse_silence_intervals(stderr: &str) -> Vec<SilenceInterval> {
    let mut intervals = Vec::new();
    let mut pending_start: Option<f64> = None;

    for line in stderr.lines() {
        if let Some(idx) = line.find("silence_start:") {
            let after = &line[idx + 15..];
            if let Ok(secs) = after.trim().parse::<f64>() {
                pending_start = Some(secs);
            }
        } else if let Some(idx) = line.find("silence_end:") {
            let after = &line[idx + 13..];
            let end_str = after
                .find('|')
                .map_or(after.trim(), |pipe| after[..pipe].trim());

            if let (Some(start_secs), Ok(end_secs)) = (pending_start.take(), end_str.parse::<f64>())
            {
                let dur = (end_secs - start_secs).max(0.0);
                intervals.push(SilenceInterval {
                    start: Timestamp::from_seconds(start_secs),
                    end: Timestamp::from_seconds(end_secs),
                    duration: Duration::from_secs_f64(dur),
                });
            }
        }
    }

    intervals
}

/// Parse ffprobe `-show_chapters` JSON into chapter markers.
fn parse_chapters(json: &serde_json::Value) -> Vec<Chapter> {
    let chapters = match json.get("chapters").and_then(|c| c.as_array()) {
        Some(c) => c,
        None => return Vec::new(),
    };

    chapters
        .iter()
        .enumerate()
        .filter_map(|(i, ch)| {
            let start = ch
                .get("start_time")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok())?;
            let end = ch
                .get("end_time")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok())?;

            let title = ch
                .get("tags")
                .and_then(|t| t.get("title"))
                .and_then(|v| v.as_str())
                .map(String::from);

            Some(Chapter {
                index: i,
                range: TimeRange::from_seconds(start, end),
                title,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_showinfo_extracts_timestamps() {
        let stderr = r#"
[Parsed_showinfo_1 @ 0x600002e30a00] n:   0 pts:      0 pts_time:0       pos:    48 fmt:yuv420p
[Parsed_showinfo_1 @ 0x600002e30a00] n:   1 pts:  48000 pts_time:1.001   pos: 12340 fmt:yuv420p
"#;
        let ts = parse_showinfo_timestamps(stderr);
        assert_eq!(ts.len(), 2);
        assert_eq!(ts[0].as_millis(), 0);
        assert_eq!(ts[1].as_millis(), 1001);
    }

    #[test]
    fn parse_keyframes_from_json() {
        let json = serde_json::json!({
            "frames": [
                { "key_frame": 1, "pts_time": "0.000000", "pict_type": "I", "pkt_size": "24000" },
                { "key_frame": 0, "pts_time": "0.033333", "pict_type": "P", "pkt_size": "1200" },
                { "key_frame": 1, "pts_time": "2.000000", "pict_type": "I", "pkt_size": "22000" },
            ]
        });
        let kf = parse_keyframes(&json);
        assert_eq!(kf.len(), 2);
        assert_eq!(kf[0].frame_number, 0);
        assert!(kf[0].picture_type.is_keyframe());
        assert_eq!(kf[0].size_bytes, Some(24000));
        assert_eq!(kf[1].frame_number, 2);
        assert_eq!(kf[1].timestamp.as_seconds(), 2.0);
    }

    #[test]
    fn parse_silence_from_stderr() {
        let stderr = r#"
[silencedetect @ 0x600002b50000] silence_start: 1.5
[silencedetect @ 0x600002b50000] silence_end: 3.2 | silence_duration: 1.7
[silencedetect @ 0x600002b50000] silence_start: 10.0
[silencedetect @ 0x600002b50000] silence_end: 11.5 | silence_duration: 1.5
"#;
        let intervals = parse_silence_intervals(stderr);
        assert_eq!(intervals.len(), 2);
        assert_eq!(intervals[0].start.as_seconds(), 1.5);
        assert_eq!(intervals[0].end.as_seconds(), 3.2);
        assert_eq!(intervals[0].midpoint().as_seconds(), 2.35);
        assert_eq!(intervals[1].start.as_seconds(), 10.0);
    }

    #[test]
    fn parse_chapters_from_json() {
        let json = serde_json::json!({
            "chapters": [
                {
                    "start_time": "0.000000",
                    "end_time": "120.000000",
                    "tags": { "title": "Introduction" }
                },
                {
                    "start_time": "120.000000",
                    "end_time": "360.000000",
                    "tags": { "title": "Main Content" }
                },
            ]
        });
        let ch = parse_chapters(&json);
        assert_eq!(ch.len(), 2);
        assert_eq!(ch[0].title.as_deref(), Some("Introduction"));
        assert_eq!(ch[0].range.duration().as_secs(), 120);
        assert_eq!(ch[1].index, 1);
    }

    #[test]
    fn parse_chapters_missing_section() {
        let json = serde_json::json!({ "format": {} });
        let ch = parse_chapters(&json);
        assert!(ch.is_empty());
    }
}
