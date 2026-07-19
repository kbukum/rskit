//! Configuration types for the `DetectScenes` operation.

use serde::{Deserialize, Serialize};

/// Configuration for scene boundary detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneDetectConfig {
    /// Scene change threshold (0.0–1.0). Lower values detect more scene changes. Default: 0.3.
    pub threshold: f32,
    /// Minimum duration in seconds between detected scenes.
    pub min_scene_duration: f64,
    /// Detection algorithm to use.
    pub method: SceneDetectMethod,
}

impl Default for SceneDetectConfig {
    fn default() -> Self {
        Self {
            threshold: 0.3,
            min_scene_duration: 1.0,
            method: SceneDetectMethod::ContentAware,
        }
    }
}

/// Scene detection algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum SceneDetectMethod {
    /// Content-aware detection using perceptual hashing.
    ContentAware,
    /// Simple threshold-based detection on pixel differences.
    Threshold,
}
