//! Stream operators for structured dataset records.

use std::pin::Pin;

use futures::Stream;
use futures_util::StreamExt as _;
use rskit_errors::AppResult;
use rskit_stream::RskitStreamExt;

use super::model::DatasetRecord;

/// Boxed stream of structured dataset records.
pub type BoxRecordStream = Pin<Box<dyn Stream<Item = AppResult<DatasetRecord>> + Send + 'static>>;

/// Project a record stream to the requested columns.
pub fn select_columns<S>(
    records: S,
    columns: Vec<String>,
) -> impl Stream<Item = AppResult<DatasetRecord>>
where
    S: Stream<Item = AppResult<DatasetRecord>> + Send + 'static,
{
    records.rmap(move |record| {
        let columns = columns.clone();
        async move { record.map(|record| record.select(&columns)) }
    })
}

/// Filter a record stream with a fallible predicate.
pub fn filter_records<S, F>(
    records: S,
    predicate: F,
) -> impl Stream<Item = AppResult<DatasetRecord>>
where
    S: Stream<Item = AppResult<DatasetRecord>> + Send + 'static,
    F: Fn(&DatasetRecord) -> AppResult<bool> + Clone + Send + Sync + 'static,
{
    records.filter_map(move |record| {
        let predicate = predicate.clone();
        async move {
            match record {
                Ok(record) => match predicate(&record) {
                    Ok(true) => Some(Ok(record)),
                    Ok(false) => None,
                    Err(error) => Some(Err(error)),
                },
                Err(error) => Some(Err(error)),
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use futures::stream;
    use futures_util::StreamExt as _;
    use rskit_errors::{AppError, ErrorCode};
    use serde_json::{Value, json};

    use super::*;

    #[tokio::test]
    async fn select_and_filter_streams_forward_errors_and_drop_false_predicates() {
        let records = stream::iter(vec![
            Ok(DatasetRecord::from_fields([
                ("keep", json!(true)),
                ("name", json!("a")),
            ])),
            Ok(DatasetRecord::from_fields([
                ("keep", json!(false)),
                ("name", json!("b")),
            ])),
            Err(AppError::new(ErrorCode::Internal, "boom")),
        ]);
        let filtered = filter_records(records, |record| {
            Ok(record.get("keep").and_then(Value::as_bool).unwrap_or(false))
        });
        let selected = select_columns(filtered, vec!["name".to_string()]);
        futures_util::pin_mut!(selected);

        assert_eq!(
            selected.next().await.unwrap().unwrap().into_json(),
            json!({"name":"a"})
        );
        assert!(selected.next().await.unwrap().is_err());
        assert!(selected.next().await.is_none());
    }
}
