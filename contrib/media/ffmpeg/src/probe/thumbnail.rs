//! Thumbnail and visual extraction methods for [`FfmpegProbe`].

use std::ffi::OsString;
use std::time::Duration;

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_media::spatial::Resolution;
use rskit_media::time::Timestamp;
use rskit_storage::FileSource;

use super::FfmpegProbe;
use crate::process::{ensure_success, run_capture, with_context};

impl FfmpegProbe {
    /// Extract a single thumbnail frame at a given timestamp.
    pub(crate) async fn extract_thumbnail(
        &self,
        source: &FileSource,
        at: Timestamp,
        resolution: Option<Resolution>,
    ) -> AppResult<FileSource> {
        let resolved = source.to_local_path().await?;
        let input_path = crate::paths::resolved_source_path(&self.config, source, resolved.path())?;
        let tmp = rskit_storage::TempFile::with_extension("jpg")?;

        let mut args = vec![
            OsString::from("-ss"),
            OsString::from(at.to_ffmpeg_time()),
            OsString::from("-i"),
            input_path.as_os_str().to_os_string(),
            OsString::from("-vframes"),
            OsString::from("1"),
        ];

        if let Some(res) = resolution {
            args.extend([
                OsString::from("-vf"),
                OsString::from(format!("scale={}:{}", res.width, res.height)),
            ]);
        }

        args.extend([OsString::from("-y"), tmp.path().as_os_str().to_os_string()]);

        let output = run_capture(self.config.ffmpeg_bin(), args, self.config.timeout)
            .await
            .map_err(|e| with_context(e, "ffmpeg thumbnail failed"))?;

        ensure_success(&output, "ffmpeg thumbnail")?;

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
        let input_path = crate::paths::resolved_source_path(&self.config, source, resolved.path())?;
        let tmp_dir = rskit_storage::TempDir::new()?;
        let pattern = tmp_dir.path().join("thumb_%04d.jpg");

        let mut vf = format!("fps=1/{}", interval.as_secs().max(1));
        if let Some(res) = resolution {
            vf.push_str(&format!(",scale={}:{}", res.width, res.height));
        }

        let output = run_capture(
            self.config.ffmpeg_bin(),
            vec![
                OsString::from("-i"),
                input_path.as_os_str().to_os_string(),
                OsString::from("-vf"),
                OsString::from(vf),
                OsString::from("-y"),
                pattern.as_os_str().to_os_string(),
            ],
            self.config.timeout,
        )
        .await
        .map_err(|e| with_context(e, "ffmpeg thumbnails failed"))?;

        ensure_success(&output, "ffmpeg thumbnails")?;

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
        let input_path = crate::paths::resolved_source_path(&self.config, source, resolved.path())?;
        let tmp = rskit_storage::TempFile::with_extension("jpg")?;

        let vf = format!(
            "fps=1/{},scale={}:{},tile={}x0",
            interval.as_secs().max(1),
            thumb_resolution.width,
            thumb_resolution.height,
            columns,
        );

        let output = run_capture(
            self.config.ffmpeg_bin(),
            vec![
                OsString::from("-i"),
                input_path.as_os_str().to_os_string(),
                OsString::from("-vf"),
                OsString::from(vf),
                OsString::from("-frames:v"),
                OsString::from("1"),
                OsString::from("-y"),
                tmp.path().as_os_str().to_os_string(),
            ],
            self.config.timeout,
        )
        .await
        .map_err(|e| with_context(e, "ffmpeg sprite_sheet failed"))?;

        ensure_success(&output, "ffmpeg sprite_sheet")?;

        Ok(tmp.into_source())
    }

    /// Generate an audio waveform image.
    pub(crate) async fn extract_waveform(
        &self,
        source: &FileSource,
        resolution: Resolution,
    ) -> AppResult<FileSource> {
        let resolved = source.to_local_path().await?;
        let input_path = crate::paths::resolved_source_path(&self.config, source, resolved.path())?;
        let tmp = rskit_storage::TempFile::with_extension("png")?;

        let output = run_capture(
            self.config.ffmpeg_bin(),
            vec![
                OsString::from("-i"),
                input_path.as_os_str().to_os_string(),
                OsString::from("-filter_complex"),
                OsString::from(format!(
                    "showwavespic=s={}x{}:colors=#4080ff",
                    resolution.width, resolution.height,
                )),
                OsString::from("-frames:v"),
                OsString::from("1"),
                OsString::from("-y"),
                tmp.path().as_os_str().to_os_string(),
            ],
            self.config.timeout,
        )
        .await
        .map_err(|e| with_context(e, "ffmpeg waveform failed"))?;

        ensure_success(&output, "ffmpeg waveform")?;

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
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| AppError::new(ErrorCode::Internal, format!("failed to read entry: {e}")))?
    {
        let p = entry.path();
        if p.extension().is_some_and(|e| e == "jpg") {
            paths.push(p);
        }
    }
    paths.sort();

    let mut results = Vec::with_capacity(paths.len());
    for p in paths {
        let data = tokio::fs::read(&p).await.map_err(|e| {
            AppError::new(ErrorCode::Internal, format!("failed to read image: {e}"))
        })?;
        results.push(FileSource::Bytes(bytes::Bytes::from(data)));
    }

    Ok(results)
}
