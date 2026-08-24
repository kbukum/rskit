//! Generic dataset loader with pipeline integration.

use crate::dataset::{DatasetManifest, Sample, load_content, load_manifest_file};
use crate::types::{BenchSample, LabelMapper};
use rskit_errors::AppResult;
use std::path::PathBuf;

/// Configuration for loading benchmark datasets from a manifest.
pub struct DatasetLoaderConfig {
    /// Manifest file name relative to the dataset directory.
    pub manifest_file: String,
    #[allow(clippy::type_complexity)]
    /// Optional predicate used to exclude manifest samples before content is read.
    pub filter: Option<Box<dyn Fn(&Sample) -> bool + Send + Sync>>,
}

impl Default for DatasetLoaderConfig {
    fn default() -> Self {
        Self {
            manifest_file: "manifest.json".to_string(),
            filter: None,
        }
    }
}

/// Loads manifest samples and maps their string labels into typed benchmark samples.
pub struct DatasetLoader<L = String> {
    dir: PathBuf,
    mapper: LabelMapper<L>,
    config: DatasetLoaderConfig,
}

impl<L: Send + Clone + 'static> DatasetLoader<L> {
    /// Creates a loader rooted at `dir` using `mapper` to convert manifest labels.
    pub fn new(dir: impl Into<PathBuf>, mapper: LabelMapper<L>) -> Self {
        Self {
            dir: dir.into(),
            mapper,
            config: DatasetLoaderConfig::default(),
        }
    }

    #[must_use]
    /// Sets the manifest file name to load from the dataset directory.
    pub fn with_manifest_file(mut self, name: impl Into<String>) -> Self {
        self.config.manifest_file = name.into();
        self
    }

    /// Loads and validates the configured dataset manifest.
    pub fn manifest(&self) -> AppResult<DatasetManifest> {
        load_manifest_file(&self.dir, &self.config.manifest_file)
    }

    /// Loads all manifest samples that pass the optional filter.
    pub fn all(&self) -> AppResult<Vec<BenchSample<L>>> {
        let manifest = self.manifest()?;
        let mut samples = Vec::new();
        for s in &manifest.samples {
            if let Some(ref filter) = self.config.filter
                && !filter(s)
            {
                continue;
            }
            let label = (self.mapper)(&s.label)?;
            let input = load_content(&self.dir, s)?;
            samples.push(BenchSample {
                id: s.id.clone(),
                input,
                label,
                source: s.source.clone(),
                metadata: s.metadata.clone(),
            });
        }
        Ok(samples)
    }

    /// Sets a predicate that filters manifest samples before loading them.
    pub fn filter<F>(mut self, predicate: F) -> Self
    where
        F: Fn(&Sample) -> bool + Send + Sync + 'static,
    {
        self.config.filter = Some(Box::new(predicate));
        self
    }
}
