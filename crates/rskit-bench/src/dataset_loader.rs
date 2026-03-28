//! Generic dataset loader with pipeline integration.

use crate::dataset::{DatasetManifest, Sample, load_content, load_manifest};
use crate::types::{BenchSample, LabelMapper};
use rskit_errors::AppResult;
use std::path::PathBuf;

pub struct DatasetLoaderConfig {
    pub manifest_file: String,
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

pub struct DatasetLoader<L = String> {
    dir: PathBuf,
    mapper: LabelMapper<L>,
    config: DatasetLoaderConfig,
}

impl<L: Send + Clone + 'static> DatasetLoader<L> {
    pub fn new(dir: impl Into<PathBuf>, mapper: LabelMapper<L>) -> Self {
        Self {
            dir: dir.into(),
            mapper,
            config: DatasetLoaderConfig::default(),
        }
    }

    pub fn with_manifest_file(mut self, name: impl Into<String>) -> Self {
        self.config.manifest_file = name.into();
        self
    }

    pub fn manifest(&self) -> AppResult<DatasetManifest> {
        load_manifest(&self.dir)
    }

    pub fn all(&self) -> AppResult<Vec<BenchSample<L>>> {
        let manifest = self.manifest()?;
        let mut samples = Vec::new();
        for s in &manifest.samples {
            if let Some(ref filter) = self.config.filter {
                if !filter(s) {
                    continue;
                }
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

    pub fn filter<F>(mut self, predicate: F) -> Self
    where
        F: Fn(&Sample) -> bool + Send + Sync + 'static,
    {
        self.config.filter = Some(Box::new(predicate));
        self
    }
}
