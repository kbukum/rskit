use std::time::Duration;

use rskit_errors::AppResult;
use rskit_media::{
    probe::{Chapter, KeyframeInfo, MediaMetadata, MediaProbe, SilenceInterval},
    spatial::Resolution,
    time::Timestamp,
};
use rskit_storage::FileSource;

use super::{FfmpegProbe, parse};

#[async_trait::async_trait]
impl MediaProbe for FfmpegProbe {
    async fn probe(&self, source: &FileSource) -> AppResult<MediaMetadata> {
        let json = self.probe_raw(source).await?;
        parse::parse_metadata(&json)
    }

    async fn thumbnail(
        &self,
        source: &FileSource,
        at: Timestamp,
        resolution: Option<Resolution>,
    ) -> AppResult<FileSource> {
        self.extract_thumbnail(source, at, resolution).await
    }

    async fn thumbnails(
        &self,
        source: &FileSource,
        interval: Duration,
        resolution: Option<Resolution>,
    ) -> AppResult<Vec<FileSource>> {
        self.extract_thumbnails(source, interval, resolution).await
    }

    async fn sprite_sheet(
        &self,
        source: &FileSource,
        interval: Duration,
        thumb_resolution: Resolution,
        columns: u32,
    ) -> AppResult<FileSource> {
        self.extract_sprite_sheet(source, interval, thumb_resolution, columns)
            .await
    }

    async fn scene_detect(&self, source: &FileSource, threshold: f64) -> AppResult<Vec<Timestamp>> {
        self.detect_scenes(source, threshold).await
    }

    async fn waveform(&self, source: &FileSource, resolution: Resolution) -> AppResult<FileSource> {
        self.extract_waveform(source, resolution).await
    }

    async fn keyframes(&self, source: &FileSource) -> AppResult<Vec<KeyframeInfo>> {
        self.extract_keyframes(source).await
    }

    async fn silence_detect(
        &self,
        source: &FileSource,
        min_duration: Duration,
        noise_threshold_db: f64,
    ) -> AppResult<Vec<SilenceInterval>> {
        self.detect_silence(source, min_duration, noise_threshold_db)
            .await
    }

    async fn chapters(&self, source: &FileSource) -> AppResult<Vec<Chapter>> {
        self.extract_chapters(source).await
    }
}
