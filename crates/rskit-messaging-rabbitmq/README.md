# rskit-messaging-rabbitmq

RabbitMQ adapter for `rskit-messaging`.

This crate provides explicit, side-effect-free registration of RabbitMQ producer and consumer factories for `Vec<u8>` payloads. It keeps AMQP dependencies out of `rskit-messaging` core; applications opt in by depending on this crate and calling `register` during composition.

```rust,ignore
use rskit_messaging::MessagingRegistry;
use rskit_messaging_rabbitmq::{register, RabbitMqConfig};

let mut registry = MessagingRegistry::<Vec<u8>>::new();
register(&mut registry, RabbitMqConfig::default())?;
```

The generic `MessageConsumer` API supports at-most-once auto-acknowledged consumption. Handler-coupled acknowledgements require a dedicated ack-capable consumer API.
