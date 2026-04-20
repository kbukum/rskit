//! Thumbnail and visual extraction methods for [`FfmpegProbe`].

use std::time::Duration;

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_file::FileSource;
use rskit_media::spatial::Resolution;
use rskit_media::time::Timestamp;

use super::FfmpegProbe;

impl FfmpegProbe {
    /// Extract a single thumbnail frame at a given timestamp.
    pub(crate) async fn extract_thumbnail(
        &self,
        source: &FileSource,
        at: Timestamp,
        resolution: Option<Resolution>,
    ) -> AppResult<FileSource> {
        let resolved = source.to_local_path().await?;
        let tmp = rskit_file::TempFile::with_extension("jpg")?;

        let mut args = vec![
            "-ss".to_string(),
            at.to_ffmpeg_time(),
            "-i".to_string(),
            resolved.path().to_string_lossy().to_string(),
            "-vframes".to_string(),
            "1".to_string(),
        ];

        if let Some(res) = resolution {
            args.extend([
                "-vf".to_string(),
                format!("scale={}:{}", res.width, res.height),
            ]);
        }

        args.extend(["-y".to_string(), tmp.path().to_string_lossy().to_string()]);

        let output = tokio::process::Command::new(self.config.ffmpeg_bin())
            .args(&args)
            .output()
            .await
            .map_err(|e| {
                AppError::new(ErrorCode::Internal, format!("ffmpeg thumbnail failed: {e}"))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::new(
                ErrorCode::Internal,
                format!("ffmpeg thumbnail failed: {stderr}"),
            ));
        }

        Ok(tmp.into_source())
    }

    /// Extract thumbnails at regular intervals.
    pub(crate) async fn extract_thumbnails(
        &self,
        source: &FileSource,
        interval: Duration,
        resolution: Option<Resolution>,
    ) -> AppResult<Vec<FileSource>> {
        let resolved = source.to_local_path().await?;
        let tmp_dir = rskit_file::TempDir::new()?;
        let pattern = tmp_dir.path().join("thumb_%04d.jpg");

        let mut vf = format!("fps=1/{}", interval.as_secs().max(1));
        if let Some(res) = resolution {
            vf.push_str(&format!(",scale={}:{}", res.width, res.height));
        }

        let output = tokio::process::Command::new(self.config.ffmpeg_bin())
            .args([
                "-i",
                &resolved.path().to_string_lossy(),
                "-vf",
                &vf,
                "-y",
                &pattern.to_string_lossy(),
            ])
            .output()
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("ffmpeg thumbnails failed: {e}"),
                )
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::new(
                ErrorCode::Internal,
                format!("ffmpeg thumbnails failed: {stderr}"),
            ));
        }

        collect_images(tmp_dir.path()).await
    }

    /// Generate a thumbnail sprite sheet (contact sheet).
    pub(crate) async fn extract_sprite_sheet(
        &self,
        source: &FileSource,
        interval: Duration,
        thumb_resolution: Resolution,
        columns: u32,
    ) -> AppResult<FileSource> {
        let resolved = source.to_local_path().await?;
        let tmp = rskit_file::TempFile::with_extension("jpg")?;

        let vf = format!(
            "fps=1/{},scale={}:{},tile={}x0",
            interval.as_secs().max(1),
            thumb_resolution.width,
            thumb_resolution.height,
            columns,
        );

        let output = tokio::process::Command::new(self.config.ffmpeg_bin())
            .args([
                "-i",
                &resolved.path().to_string_lossy(),
                "-vf",
                &vf,
                "-frames:v",
                "1",
                "-y",
                &tmp.path().to_string_lossy(),
            ])
            .output()
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("ffmpeg sprite_sheet failed: {e}"),
                )
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::new(
                ErrorCode::Internal,
                format!("ffmpeg sprite_sheet failed: {stderr}"),
            ));
        }

        Ok(tmp.into_source())
    }

    /// Generate an audio waveform image.
    pub(crate) async fn extract_waveform(
        &self,
        source: &FileSource,
        resolution: Resolution,
    ) -> AppResult<FileSource> {
        let resolved = source.to_local_path().await?;
        let tmp = rskit_file::TempFile::with_extension("png")?;

        let output = tokio::process::Command::new(self.config.ffmpeg_bin())
            .args([
                "-i",
                &resolved.path().to_string_lossy(),
                "-filter_complex",
                &format!(
                    "showwavespic=s={}x{}:colors=#4080ff",
                    resolution.width, resolution.height,
                ),
                "-frames:v",
                "1",
                "-y",
                &tmp.path().to_string_lossy(),
            ])
            .output()
            .await
            .map_err(|e| {
                AppError::new(ErrorCode::Internal, format!("ffmpeg waveform failed: {e}"))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::new(
                ErrorCode::Internal,
                format!("ffmpeg waveform failed: {stderr}"),
            ));
        }

        Ok(tmp.into_source())
    }
}

/// Collect generated image files from a directory, sorted by name.
async fn collect_images(dir: &std::path::Path) -> AppResult<Vec<FileSource>> {
    let mut entries = tokio::fs::read_dir(dir).await.map_err(|e| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to read output dir: {e}"),
        )
    })?;

    let mut paths = Vec::new();
    while let Some(entry) = entries.next_entry().await.map_err(|e| {
        AppError::new(ErrorCode::Internal, format!("failed to read entry: {e}"))
    })? {
        let p = entry.path();
        if p.extension().is_some_and(|e| e == "jpg") {
            paths.push(p);
        }
    }
    paths.sort();

    let mut results = Vec::with_capacity(paths.len());
    for p in paths {
        let data = tokio::fs::read(&p).await.map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to read image: {e}"),
            )
        })?;
        results.push(FileSource::Bytes(bytes::Bytes::from(data)));
    }

    Ok(results)
}
