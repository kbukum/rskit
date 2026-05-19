//! Bridge adapters from messaging traits to [`rskit_provider`] traits.
//!
//! - [`ProducerSink`] wraps a [`MessageProducer`] as a [`Sink`].
//! - [`ConsumerStream`] wraps a [`MessageConsumer`] as a [`Stream`].

use std::sync::Arc;

use async_trait::async_trait;
use rskit_errors::AppResult;
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

#[async_trait]
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
/// Calling [`Stream::execute`] returns a bounded stream of consumed messages.
/// The stream yields at most `max_messages` items before completing.
pub struct ConsumerStream<T: Send + Sync + 'static> {
    name: &'static str,
    consumer: Arc<dyn MessageConsumer<T>>,
    max_messages: usize,
}

#[async_trait]
impl<T: Send + Sync + 'static> Provider for ConsumerStream<T> {
    fn name(&self) -> &'static str {
        self.name
    }
}

#[async_trait]
impl<T: Send + Sync + Clone + 'static> Stream<(), Message<T>> for ConsumerStream<T> {
    async fn execute(&self, _input: ()) -> AppResult<BoxStream<Message<T>>> {
        let consumer = Arc::clone(&self.consumer);
        let max_messages = self.max_messages;
        let stream = async_stream::try_stream! {
            for _ in 0..max_messages {
                let msg = consumer.recv().await?;
                yield msg;
            }
        };
        Ok(Box::pin(stream))
    }
}

/// Create a [`ConsumerStream`] that yields messages from the given consumer.
#[must_use]
pub fn consumer_as_stream<T: Send + Sync + 'static>(
    name: &'static str,
    consumer: Arc<dyn MessageConsumer<T>>,
) -> ConsumerStream<T> {
    consumer_as_bounded_stream(name, consumer, 1024)
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
        max_messages: max_messages.max(1),
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

        let received = consumer.recv().await.unwrap();
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

        let received = consumer.recv().await.unwrap();
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
