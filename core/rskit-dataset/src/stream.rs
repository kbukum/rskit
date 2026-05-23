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
