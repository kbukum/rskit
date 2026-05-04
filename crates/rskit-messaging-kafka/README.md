# rskit-messaging-kafka

Kafka adapter for `rskit-messaging`.

This crate provides explicit, side-effect-free registration of Kafka producer and consumer factories for `Vec<u8>` payloads. It keeps broker SDK dependencies out of `rskit-messaging` core; applications opt in by depending on this crate and calling `register` during composition.

```rust,ignore
use rskit_messaging::MessagingRegistry;
use rskit_messaging_kafka::{register, KafkaConfig};

let mut registry = MessagingRegistry::<Vec<u8>>::new();
register(&mut registry, KafkaConfig::default())?;
```

Configure broker endpoints, delivery semantics, security, compression, and consumer groups through `KafkaConfig`.
