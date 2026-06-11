//! Source trait — pull data from any origin.

use crate::DataItem;
use rskit_errors::AppResult;
use std::pin::Pin;
use tokio_util::sync::CancellationToken;

/// Boxed dataset stream emitted by a source.
pub type BoxDataStream = Pin<Box<dyn futures::Stream<Item = AppResult<DataItem>> + Send + 'static>>;

/// Protocol for dataset sources.
pub trait Source: Send + Sync {
    /// Stable source identifier used in manifests.
    fn name(&self) -> &str;

    /// Human-readable source label.
    fn display_name(&self) -> &str {
        self.name().rsplit('/').next().unwrap_or(self.name())
    }

    /// Stream items from this source.
    fn stream(self: Box<Self>, cancel: CancellationToken) -> BoxDataStream;

    /// Stable cache key describing this source's configured inputs.
    fn cache_key(&self) -> serde_json::Value;
    /// Optional maximum number of items this source will emit.
    fn max_items(&self) -> Option<usize>;

    /// Configure resume state before streaming.
    fn set_resume_state(&mut self, _offset: usize, _already_fetched: usize) {}
}

#[cfg(test)]
mod tests {
    use futures::stream;
    use serde_json::json;

    use super::*;

    struct StaticSource {
        name: String,
        resume: Option<(usize, usize)>,
    }

    struct DefaultResumeSource;

    impl Source for DefaultResumeSource {
        fn name(&self) -> &str {
            "default"
        }

        fn stream(self: Box<Self>, _cancel: CancellationToken) -> BoxDataStream {
            Box::pin(stream::empty())
        }

        fn cache_key(&self) -> serde_json::Value {
            json!("default")
        }

        fn max_items(&self) -> Option<usize> {
            None
        }
    }

    impl Source for StaticSource {
        fn name(&self) -> &str {
            &self.name
        }

        fn stream(self: Box<Self>, _cancel: CancellationToken) -> BoxDataStream {
            Box::pin(stream::empty())
        }

        fn cache_key(&self) -> serde_json::Value {
            json!({"name": self.name})
        }

        fn max_items(&self) -> Option<usize> {
            Some(0)
        }

        fn set_resume_state(&mut self, offset: usize, already_fetched: usize) {
            self.resume = Some((offset, already_fetched));
        }
    }

    #[test]
    fn source_defaults_derive_display_name_and_allow_resume_state() {
        let mut source = StaticSource {
            name: "fixtures/images".to_string(),
            resume: None,
        };

        assert_eq!(source.name(), "fixtures/images");
        assert_eq!(source.display_name(), "images");
        assert_eq!(source.cache_key(), json!({"name": "fixtures/images"}));
        assert_eq!(source.max_items(), Some(0));

        source.set_resume_state(10, 3);
        assert_eq!(source.resume, Some((10, 3)));

        let stream = Box::new(source).stream(CancellationToken::new());
        drop(stream);

        let mut default_resume = DefaultResumeSource;
        default_resume.set_resume_state(1, 1);
        assert_eq!(default_resume.display_name(), "default");
    }
}
