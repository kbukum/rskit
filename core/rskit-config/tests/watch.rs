#![cfg(feature = "watch")]

//! Behavioral tests for the `watch` feature: a `ConfigWatch` change event drives
//! a consumer to re-decode, and cancellation terminates the stream.

use futures::StreamExt;
use rskit_config::{
    ConfigChange, ConfigLoader, ConfigMapSource, ConfigSink, ConfigWatch, InMemoryConfigSink,
};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Deserialize)]
struct AppSettings {
    port: u16,
}

/// Re-run the load pipeline against the sink's current state and decode.
fn redecode(sink: &InMemoryConfigSink) -> AppSettings {
    let port = sink
        .get("port")
        .map(|v| v.expose().to_string())
        .unwrap_or_default();
    let source = ConfigMapSource::new().with_value("port", port);
    ConfigLoader::custom()
        .with_source(source)
        .load::<AppSettings>()
        .expect("decode current config")
}

#[tokio::test]
async fn change_event_triggers_redecode() {
    let sink = InMemoryConfigSink::new();
    sink.set("port", rskit_config::SecretString::new("8080"))
        .unwrap();

    let cancel = CancellationToken::new();
    let mut changes = sink.watch(cancel.clone()).unwrap();

    // A write emits a typed Set event carrying only the key.
    sink.set("port", rskit_config::SecretString::new("8081"))
        .unwrap();

    let event = changes.next().await.expect("a change event");
    assert_eq!(
        event,
        ConfigChange::Set {
            key: "port".to_string()
        }
    );

    // The consumer reacts by re-decoding the new state.
    let settings = redecode(&sink);
    assert_eq!(settings.port, 8081);

    cancel.cancel();
}

#[tokio::test]
async fn remove_emits_removed_event() {
    let sink = InMemoryConfigSink::new();
    sink.set("k", rskit_config::SecretString::new("v")).unwrap();

    let cancel = CancellationToken::new();
    let mut changes = sink.watch(cancel.clone()).unwrap();

    sink.remove("k").unwrap();

    let event = changes.next().await.expect("a change event");
    assert_eq!(
        event,
        ConfigChange::Removed {
            key: "k".to_string()
        }
    );
    cancel.cancel();
}

#[tokio::test]
async fn cancellation_terminates_stream() {
    let sink = InMemoryConfigSink::new();
    let cancel = CancellationToken::new();
    let mut changes = sink.watch(cancel.clone()).unwrap();

    cancel.cancel();

    // Once cancelled, the stream completes rather than hanging.
    assert!(changes.next().await.is_none());
}

#[tokio::test]
async fn multiple_subscribers_each_receive_changes() {
    let sink = InMemoryConfigSink::new();
    let cancel = CancellationToken::new();
    let mut first = sink.watch(cancel.clone()).unwrap();
    let mut second = sink.watch(cancel.clone()).unwrap();

    sink.set("k", rskit_config::SecretString::new("v")).unwrap();

    let expected = ConfigChange::Set {
        key: "k".to_string(),
    };
    assert_eq!(first.next().await.unwrap(), expected);
    assert_eq!(second.next().await.unwrap(), expected);
    cancel.cancel();
}
