//! Bridge adapters from messaging traits to [`rskit_provider`] traits.
//!
//! - [`ProducerSink`] wraps a [`MessageProducer`] as a [`Sink`].
//! - [`ConsumerStream`] wraps a [`MessageConsumer`] as a [`Stream`].

use std::sync::Arc;

use async_trait::async_trait;
use rskit_errors::{AppResult, ErrorCode};
use rskit_provider::traits::{BoxStream, Provider, Sink, Stream};

use crate::message::Message;
use crate::traits::{MessageConsumer, MessageProducer};

// ─── ProducerSink ────────────────────────────────────────────────────────────

/// Wraps a [`MessageProducer`] as a [`Sink<Message<T>>`](Sink).
///
/// Each call to [`Sink::send`] publishes the message to the configured topic.
pub struct ProducerSink<T: Send + Sync + 'static> {
    name: &'static str,
    producer: Arc<dyn MessageProducer<T>>,
    topic: String,
}

#[async_trait]
impl<T: Send + Sync + 'static> Provider for ProducerSink<T> {
    fn name(&self) -> &'static str {
        self.name
    }
}

impl<T: Send + Sync + 'static> Sink<Message<T>> for ProducerSink<T> {
    async fn send(&self, mut input: Message<T>) -> AppResult<()> {
        if input.topic.is_empty() {
            input.topic.clone_from(&self.topic);
        }
        self.producer.send(input).await
    }
}

/// Create a [`ProducerSink`] that publishes messages via the given producer.
///
/// The `topic` is applied as a default when a message has no topic set.
#[must_use]
pub fn producer_as_sink<T: Send + Sync + 'static>(
    name: &'static str,
    producer: Arc<dyn MessageProducer<T>>,
    topic: String,
) -> ProducerSink<T> {
    ProducerSink {
        name,
        producer,
        topic,
    }
}

// ─── ConsumerStream ──────────────────────────────────────────────────────────

/// Wraps a [`MessageConsumer`] as a [`Stream<(), Message<T>>`](Stream).
///
/// Calling [`Stream::execute`] returns a backpressure-aware stream that receives
/// one message at a time. Use [`consumer_as_bounded_stream`] when the stream
/// should complete after a fixed number of messages.
pub struct ConsumerStream<T: Send + Sync + 'static> {
    name: &'static str,
    consumer: Arc<dyn MessageConsumer<T>>,
    max_messages: Option<usize>,
}

#[async_trait]
impl<T: Send + Sync + 'static> Provider for ConsumerStream<T> {
    fn name(&self) -> &'static str {
        self.name
    }
}

impl<T: Send + Sync + Clone + 'static> Stream<(), Message<T>> for ConsumerStream<T> {
    async fn execute(&self, _input: ()) -> AppResult<BoxStream<Message<T>>> {
        let consumer = Arc::clone(&self.consumer);
        let max_messages = self.max_messages;
        let stream = async_stream::try_stream! {
            match max_messages {
                Some(max_messages) => {
                    let mut delivered = 0;
                    while delivered < max_messages {
                        match consumer.recv(std::time::Duration::from_secs(1)).await {
                            Ok(msg) => {
                                yield msg;
                                delivered += 1;
                            }
                            Err(e) if e.code() == ErrorCode::Timeout => {}
                            Err(e) => Err(e)?,
                        }
                    }
                }
                None => loop {
                    match consumer.recv(std::time::Duration::from_secs(1)).await {
                        Ok(msg) => yield msg,
                        Err(e) if e.code() == ErrorCode::Timeout => {}
                        Err(e) => Err(e)?,
                    }
                },
            }
        };
        let stream: BoxStream<Message<T>> = Box::pin(stream);
        Ok(stream)
    }
}

/// Create a [`ConsumerStream`] that yields messages from the given consumer.
#[must_use]
pub fn consumer_as_stream<T: Send + Sync + 'static>(
    name: &'static str,
    consumer: Arc<dyn MessageConsumer<T>>,
) -> ConsumerStream<T> {
    ConsumerStream {
        name,
        consumer,
        max_messages: None,
    }
}

/// Create a [`ConsumerStream`] that yields at most `max_messages` messages.
#[must_use]
pub fn consumer_as_bounded_stream<T: Send + Sync + 'static>(
    name: &'static str,
    consumer: Arc<dyn MessageConsumer<T>>,
    max_messages: usize,
) -> ConsumerStream<T> {
    ConsumerStream {
        name,
        consumer,
        max_messages: Some(max_messages),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use futures::StreamExt;

    use crate::memory::InMemoryBroker;
    use crate::traits::{MessageConsumer, MessageProducer};

    use super::*;

    #[tokio::test]
    async fn producer_sink_sends_message() {
        let broker = InMemoryBroker::<String>::new(16);
        let producer = Arc::new(broker.producer());
        let consumer = broker.consumer();
        consumer.subscribe(&["sink-topic"]).await.unwrap();

        let sink = producer_as_sink("test-sink", producer, "sink-topic".into());
        let msg = Message::new("sink-topic", "hello".into());
        Sink::send(&sink, msg).await.unwrap();

        let received = consumer
            .recv(std::time::Duration::from_secs(1))
            .await
            .unwrap();
        assert_eq!(received.topic, "sink-topic");
        assert_eq!(received.payload, "hello");
    }

    #[tokio::test]
    async fn producer_sink_applies_default_topic() {
        let broker = InMemoryBroker::<String>::new(16);
        let producer = Arc::new(broker.producer());
        let consumer = broker.consumer();
        consumer.subscribe(&["default-t"]).await.unwrap();

        let sink = producer_as_sink("test-sink", producer, "default-t".into());
        let mut msg = Message::new("", "data".into());
        msg.topic = String::new();
        Sink::send(&sink, msg).await.unwrap();

        let received = consumer
            .recv(std::time::Duration::from_secs(1))
            .await
            .unwrap();
        assert_eq!(received.topic, "default-t");
    }

    #[tokio::test]
    async fn consumer_stream_yields_messages() {
        let broker = InMemoryBroker::<String>::new(16);
        let producer = broker.producer();
        let consumer = Arc::new(broker.consumer());
        consumer.subscribe(&["stream-t"]).await.unwrap();

        let cs = consumer_as_stream("test-stream", consumer);

        // Send messages before opening the stream
        producer
            .send(Message::new("stream-t", "a".into()))
            .await
            .unwrap();
        producer
            .send(Message::new("stream-t", "b".into()))
            .await
            .unwrap();

        let stream = cs.execute(()).await.unwrap();
        let items: Vec<_> = tokio::time::timeout(Duration::from_millis(200), async {
            stream.take(2).collect::<Vec<_>>().await
        })
        .await
        .unwrap();

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].as_ref().unwrap().payload, "a");
        assert_eq!(items[1].as_ref().unwrap().payload, "b");
    }

    #[tokio::test]
    async fn bounded_consumer_stream_completes_after_limit() {
        let broker = InMemoryBroker::<String>::new(16);
        let producer = broker.producer();
        let consumer = Arc::new(broker.consumer());
        consumer.subscribe(&["stream-t"]).await.unwrap();

        let cs = consumer_as_bounded_stream("test-stream", consumer, 2);

        for value in ["a", "b", "c"] {
            producer
                .send(Message::new("stream-t", value.to_owned()))
                .await
                .unwrap();
        }

        let stream = cs.execute(()).await.unwrap();
        let items = stream.collect::<Vec<_>>().await;

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].as_ref().unwrap().payload, "a");
        assert_eq!(items[1].as_ref().unwrap().payload, "b");
    }

    #[tokio::test]
    async fn bounded_consumer_stream_with_zero_limit_completes_immediately() {
        let broker = InMemoryBroker::<String>::new(16);
        let consumer = Arc::new(broker.consumer());
        consumer.subscribe(&["stream-t"]).await.unwrap();

        let cs = consumer_as_bounded_stream("test-stream", consumer, 0);
        let stream = cs.execute(()).await.unwrap();
        let items = tokio::time::timeout(Duration::from_millis(50), stream.collect::<Vec<_>>())
            .await
            .unwrap();

        assert!(items.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn consumer_stream_survives_idle_recv_timeouts() {
        let broker = InMemoryBroker::<String>::new(16);
        let producer = broker.producer();
        let consumer = Arc::new(broker.consumer());
        consumer.subscribe(&["idle-t"]).await.unwrap();

        let cs = consumer_as_stream("idle-stream", consumer);
        let stream = cs.execute(()).await.unwrap();
        let collector = tokio::spawn(async move { stream.take(1).collect::<Vec<_>>().await });

        // Let several per-recv timeouts elapse with no traffic; the stream must
        // keep polling instead of terminating on the first idle second.
        tokio::time::sleep(Duration::from_secs(3)).await;
        producer
            .send(Message::new("idle-t", "late".into()))
            .await
            .unwrap();

        let items = tokio::time::timeout(Duration::from_secs(5), collector)
            .await
            .expect("stream terminated instead of surviving idle recv timeouts")
            .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].as_ref().unwrap().payload, "late");
    }

    #[tokio::test]
    async fn producer_sink_has_provider_name() {
        let broker = InMemoryBroker::<String>::new(4);
        let producer = Arc::new(broker.producer());
        let sink = producer_as_sink("my-sink", producer, "t".into());
        assert_eq!(Provider::name(&sink), "my-sink");
    }

    #[tokio::test]
    async fn consumer_stream_has_provider_name() {
        let broker = InMemoryBroker::<String>::new(4);
        let consumer = Arc::new(broker.consumer());
        let cs = consumer_as_stream("my-cs", consumer);
        assert_eq!(Provider::name(&cs), "my-cs");
    }
}
