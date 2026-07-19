# rskit-messaging-rabbitmq

RabbitMQ adapter for `rskit-messaging`.

This crate provides explicit, side-effect-free registration of RabbitMQ producer
and consumer factories for `Vec<u8>` payloads.
It keeps AMQP dependencies out of `rskit-messaging` core;
applications opt in by depending on this crate and calling `register` during composition.

```rust,ignore
use rskit_messaging::MessagingRegistry;
use rskit_messaging_rabbitmq::{Config, register};

let mut registry = MessagingRegistry::<Vec<u8>>::new();
register(&mut registry, Config::default())?;
```

The generic `MessageConsumer` API currently uses auto-acknowledged, at-most-once delivery.
`Config::validate` requires `commit_strategy=auto`, `auto_ack=true`, and DLQ disabled;
use middleware or a future ack-capable consumer API for retries, post-handler acknowledgements,
or DLQ routing.
