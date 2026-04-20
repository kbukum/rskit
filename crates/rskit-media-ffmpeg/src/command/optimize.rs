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
    pub fn optimize_ops(ops: &[MediaOp]) -> Vec<MediaOp> {
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
    pub fn validate_ops(ops: &[MediaOp]) -> AppResult<()> {
        let mut has_strip_audio = false;
        let mut has_strip_video = false;
        let mut has_audio_op = false;
        let mut has_video_op = false;
        let mut extract_count = 0;
        let mut extract_many_count = 0;
        let mut concat_count = 0;
        let mut overlay_count = 0;

        for op in ops {
            match op {
                MediaOp::StripAudio => has_strip_audio = true,
                MediaOp::StripVideo => has_strip_video = true,
                MediaOp::Volume(_)
                | MediaOp::NormalizeAudio
                | MediaOp::MixAudio(_)
                | MediaOp::ReplaceAudio(_) => has_audio_op = true,
                MediaOp::Resize(_)
                | MediaOp::Crop(_)
                | MediaOp::Rotate(_)
                | MediaOp::Flip(_)
                | MediaOp::Pad(_)
                | MediaOp::BurnSubtitles(_) => has_video_op = true,
                MediaOp::Overlay(_) => {
                    has_video_op = true;
                    overlay_count += 1;
                }
                MediaOp::Extract(_) => extract_count += 1,
                MediaOp::ExtractMany(_) => extract_many_count += 1,
                MediaOp::Concat(_) => concat_count += 1,
                MediaOp::Filter(f) => match f.target {
                    rskit_media::filter::FilterTarget::Video => has_video_op = true,
                    rskit_media::filter::FilterTarget::Audio => has_audio_op = true,
                },
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
