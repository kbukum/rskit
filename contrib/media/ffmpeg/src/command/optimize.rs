//! Operation optimization and pre-flight validation.
//!
//! Pure functions that refine a `MediaOp` list before FFmpeg compilation:
//! - [`FfmpegCommand::optimize_ops`] merges/removes redundant operations
//! - [`FfmpegCommand::validate_ops`] catches invalid combinations early

use rskit_errors::AppResult;
use rskit_media::ops::MediaOp;

use super::FfmpegCommand;

impl FfmpegCommand {
    /// Optimize a list of operations by removing redundancies.
    ///
    /// - Consecutive resizes: keep only the last one
    /// - Consecutive crops: keep only the last one
    /// - Multiple volume adjustments: multiply factors
    /// - Speed(1.0): remove (no-op)
    /// - Multiple consecutive Extract: keep only the last
    pub(crate) fn optimize_ops(ops: &[MediaOp]) -> Vec<MediaOp> {
        let mut result: Vec<MediaOp> = Vec::with_capacity(ops.len());

        for op in ops {
            match op {
                MediaOp::Resize(_) => {
                    // If the last op is also a resize, replace it
                    if matches!(result.last(), Some(MediaOp::Resize(_))) {
                        result.pop();
                    }
                    result.push(op.clone());
                }
                MediaOp::Crop(_) => {
                    if matches!(result.last(), Some(MediaOp::Crop(_))) {
                        result.pop();
                    }
                    result.push(op.clone());
                }
                MediaOp::Volume(factor) => {
                    if let Some(MediaOp::Volume(prev)) = result.last_mut() {
                        *prev *= factor;
                    } else {
                        result.push(op.clone());
                    }
                }
                MediaOp::Speed(factor) => {
                    // Skip no-op speed changes
                    if (*factor - 1.0).abs() < f64::EPSILON {
                        continue;
                    }
                    if let Some(MediaOp::Speed(prev)) = result.last_mut() {
                        *prev *= factor;
                    } else {
                        result.push(op.clone());
                    }
                }

                _ => result.push(op.clone()),
            }
        }

        result
    }

    /// Pre-flight validation of operations before spawning FFmpeg.
    ///
    /// Catches invalid combinations that would cause FFmpeg to fail with
    /// cryptic errors or produce incorrect output.
    pub(crate) fn validate_ops(ops: &[MediaOp]) -> AppResult<()> {
        let mut has_strip_audio = false;
        let mut has_strip_video = false;
        let mut has_audio_op = false;
        let mut has_video_op = false;
        let mut extract_count = 0;
        let mut extract_many_count = 0;
        let mut concat_count = 0;
        let mut overlay_count = 0;

        for op in ops {
            has_audio_op |= op.requires_audio_track();
            has_video_op |= op.requires_video_track();

            match op {
                MediaOp::StripAudio => has_strip_audio = true,
                MediaOp::StripVideo => has_strip_video = true,
                MediaOp::Overlay(_) => overlay_count += 1,
                MediaOp::Extract(_) => extract_count += 1,
                MediaOp::ExtractMany(_) => extract_many_count += 1,
                MediaOp::Concat(_) => concat_count += 1,
                _ => {}
            }
        }

        if has_strip_audio && has_audio_op {
            return Err(rskit_errors::AppError::new(
                rskit_errors::ErrorCode::InvalidInput,
                "Cannot apply audio operations after stripping audio (StripAudio + audio filter/volume/mix)",
            ));
        }

        if has_strip_video && has_video_op {
            return Err(rskit_errors::AppError::new(
                rskit_errors::ErrorCode::InvalidInput,
                "Cannot apply video operations after stripping video (StripVideo + video filter/resize/crop)",
            ));
        }

        if extract_count > 1 {
            return Err(rskit_errors::AppError::new(
                rskit_errors::ErrorCode::InvalidInput,
                "Multiple Extract ops found — use ExtractMany for multi-segment extraction",
            ));
        }

        if extract_many_count > 1 {
            return Err(rskit_errors::AppError::new(
                rskit_errors::ErrorCode::InvalidInput,
                "Multiple ExtractMany ops not supported",
            ));
        }

        if (extract_count > 0 || extract_many_count > 0) && concat_count > 0 {
            return Err(rskit_errors::AppError::new(
                rskit_errors::ErrorCode::InvalidInput,
                "Cannot combine Extract/ExtractMany with Concat — these use conflicting input strategies",
            ));
        }

        if overlay_count > 1 || (overlay_count > 0 && concat_count > 0) {
            // Multiple complex_filter ops would overwrite each other
            return Err(rskit_errors::AppError::new(
                rskit_errors::ErrorCode::InvalidInput,
                "Only one complex-filter operation allowed per pipeline (overlay, concat, or multi-segment extract)",
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_rejects_audio_strip_before_dual_track_speed() {
        let err = FfmpegCommand::validate_ops(&[MediaOp::StripAudio, MediaOp::Speed(2.0)])
            .expect_err("speed compiles audio filters and must not follow StripAudio");
        assert_eq!(err.code(), rskit_errors::ErrorCode::InvalidInput);
    }

    #[test]
    fn validate_rejects_video_strip_before_dual_track_reverse() {
        let err = FfmpegCommand::validate_ops(&[MediaOp::StripVideo, MediaOp::Reverse])
            .expect_err("reverse compiles video filters and must not follow StripVideo");
        assert_eq!(err.code(), rskit_errors::ErrorCode::InvalidInput);
    }

    #[test]
    fn validate_rejects_video_strip_before_extract_many() {
        let err = FfmpegCommand::validate_ops(&[
            MediaOp::StripVideo,
            MediaOp::ExtractMany(vec![rskit_media::time::Segment::new(
                rskit_media::time::TimeRange::from_seconds(0.0, 1.0),
            )]),
        ])
        .expect_err(
            "multi-segment extract compiles video concat pads and must not follow StripVideo",
        );
        assert_eq!(err.code(), rskit_errors::ErrorCode::InvalidInput);
    }

    #[test]
    fn validate_rejects_video_strip_before_concat() {
        let err = FfmpegCommand::validate_ops(&[
            MediaOp::StripVideo,
            MediaOp::Concat(rskit_media::ops::ConcatOp {
                source: rskit_storage::FileSource::from_path("next.mp4"),
                transition: None,
            }),
        ])
        .expect_err("concat compiles video pads and must not follow StripVideo");
        assert_eq!(err.code(), rskit_errors::ErrorCode::InvalidInput);
    }

    #[test]
    fn validate_rejects_video_strip_before_replace_audio() {
        let err = FfmpegCommand::validate_ops(&[
            MediaOp::StripVideo,
            MediaOp::ReplaceAudio(rskit_media::ops::ReplaceAudioOp {
                audio_source: rskit_storage::FileSource::from_path("audio.wav"),
                offset: None,
            }),
        ])
        .expect_err("replace-audio maps the primary video stream and must not follow StripVideo");
        assert_eq!(err.code(), rskit_errors::ErrorCode::InvalidInput);
    }

    #[test]
    fn optimize_merges_consecutive_operations() {
        let ops = FfmpegCommand::optimize_ops(&[
            MediaOp::Resize(rskit_media::ops::ResizeOp {
                resolution: rskit_media::spatial::Resolution::new(320, 240),
                mode: rskit_media::ops::ResizeMode::Exact,
            }),
            MediaOp::Resize(rskit_media::ops::ResizeOp {
                resolution: rskit_media::spatial::Resolution::new(640, 480),
                mode: rskit_media::ops::ResizeMode::Exact,
            }),
            MediaOp::Crop(rskit_media::ops::CropRegion {
                x: 0,
                y: 0,
                width: 10,
                height: 10,
            }),
            MediaOp::Crop(rskit_media::ops::CropRegion {
                x: 1,
                y: 2,
                width: 20,
                height: 30,
            }),
            MediaOp::Volume(0.5),
            MediaOp::Volume(2.0),
            MediaOp::Speed(1.0),
            MediaOp::Speed(2.0),
            MediaOp::Speed(0.5),
        ]);

        assert_eq!(ops.len(), 4);
        assert!(matches!(&ops[0], MediaOp::Resize(op) if op.resolution.width == 640));
        assert!(matches!(&ops[1], MediaOp::Crop(op) if op.x == 1 && op.height == 30));
        assert!(matches!(ops[2], MediaOp::Volume(factor) if (factor - 1.0).abs() < f64::EPSILON));
        assert!(matches!(ops[3], MediaOp::Speed(factor) if (factor - 1.0).abs() < f64::EPSILON));
    }

    #[test]
    fn validate_rejects_conflicting_extract_and_overlay_combinations() {
        let range = rskit_media::time::TimeRange::from_seconds(0.0, 1.0);
        let segment = rskit_media::time::Segment::new(range);
        let concat = MediaOp::Concat(rskit_media::ops::ConcatOp {
            source: rskit_storage::FileSource::from_path("next.mp4"),
            transition: None,
        });
        let overlay = MediaOp::Overlay(rskit_media::ops::OverlayOp {
            source: rskit_storage::FileSource::from_path("overlay.png"),
            position: rskit_media::ops::OverlayPosition::Center,
            opacity: 1.0,
            time_range: None,
            scale: None,
        });

        for ops in [
            vec![MediaOp::Extract(range), MediaOp::Extract(range)],
            vec![
                MediaOp::ExtractMany(vec![segment.clone()]),
                MediaOp::ExtractMany(vec![segment.clone()]),
            ],
            vec![MediaOp::Extract(range), concat.clone()],
            vec![MediaOp::ExtractMany(vec![segment]), concat.clone()],
            vec![overlay.clone(), overlay],
            vec![
                concat,
                MediaOp::Overlay(rskit_media::ops::OverlayOp {
                    source: rskit_storage::FileSource::from_path("overlay.png"),
                    position: rskit_media::ops::OverlayPosition::Center,
                    opacity: 1.0,
                    time_range: None,
                    scale: None,
                }),
            ],
        ] {
            let err = FfmpegCommand::validate_ops(&ops).unwrap_err();
            assert_eq!(err.code(), rskit_errors::ErrorCode::InvalidInput);
        }
    }
}
