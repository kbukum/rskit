//! Stream adapters for composing dataset items with `rskit-pipeline`.

use futures::Stream;
use rskit_errors::AppResult;
use rskit_pipeline::RskitStreamExt;

use crate::{DataItem, DatasetLimits, Transform};

/// Extension methods for streams of dataset items.
pub trait DatasetStreamExt: Stream<Item = AppResult<DataItem>> + Sized + Send + 'static {
    /// Apply a fallible dataset transform inside a canonical `rskit-pipeline` stream.
    fn apply_dataset_transform<T>(
        self,
        transform: T,
        limits: DatasetLimits,
    ) -> impl Stream<Item = AppResult<Option<DataItem>>> + Send + 'static
    where
        T: Transform + Clone + Send + Sync + 'static,
    {
        self.rmap(move |item| {
            let transform = transform.clone();
            async move {
                let item = item?;
                transform.apply(item, &limits)
            }
        })
    }
}

impl<S> DatasetStreamExt for S where S: Stream<Item = AppResult<DataItem>> + Sized + Send + 'static {}

#[cfg(test)]
mod tests {
    use futures::{StreamExt, stream};
    use rskit_errors::{AppError, ErrorCode};

    use super::*;
    use crate::{DataItem, Label, MediaType};

    #[derive(Clone)]
    struct FilterBySource {
        allowed: &'static str,
    }

    impl Transform for FilterBySource {
        fn name(&self) -> &str {
            "filter-by-source"
        }

        fn apply(&self, item: DataItem, _limits: &DatasetLimits) -> AppResult<Option<DataItem>> {
            if item.source_name == self.allowed {
                Ok(Some(item))
            } else {
                Ok(None)
            }
        }
    }

    #[tokio::test]
    async fn dataset_stream_transform_maps_items_and_forwards_errors() {
        let transform = FilterBySource { allowed: "keep" };
        assert_eq!(transform.name(), "filter-by-source");
        let keep = DataItem::new(vec![1], Label::Real, MediaType::Text, "keep").unwrap();
        let drop = DataItem::new(vec![2], Label::AiGenerated, MediaType::Text, "drop").unwrap();
        let input = stream::iter([
            Ok(keep),
            Ok(drop),
            Err(AppError::new(ErrorCode::Internal, "boom")),
        ]);

        let output = input
            .apply_dataset_transform(transform, DatasetLimits::default())
            .collect::<Vec<_>>()
            .await;

        assert!(output[0].as_ref().unwrap().is_some());
        assert!(output[1].as_ref().unwrap().is_none());
        assert_eq!(output[2].as_ref().unwrap_err().code(), ErrorCode::Internal);
    }
}
