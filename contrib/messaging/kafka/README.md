# rskit-messaging-kafka

Kafka adapter for `rskit-messaging`.

This crate provides explicit, side-effect-free registration of Kafka producer and consumer factories for `Vec<u8>` payloads. It keeps broker SDK dependencies out of `rskit-messaging` core; applications opt in by depending on this crate and calling `register` during composition.

```rust,ignore
use rskit_messaging::MessagingRegistry;
use rskit_messaging_kafka::{Config, register};

let mut registry = MessagingRegistry::<Vec<u8>>::new();
register(&mut registry, Config::default())?;
```

Configure broker endpoints, at-most-once or at-least-once delivery semantics, security, compression, and consumer groups through `Config`. The direct consumer currently requires `commit_strategy=auto`, and DLQ routing is left to middleware.

## Timeouts, batching, and backpressure

Consumers require an explicit receive timeout (`recv(timeout)`) so calls are always bounded. `send_batch` and `publish_batch` enqueue the whole batch before awaiting delivery, allowing librdkafka to produce it as a configured batch; each delivery is bounded by the same 5s wall-clock timeout as `send`, returning `ErrorCode::Timeout` if the broker never reports delivery. Producer buffering is bounded by `Config::queue_capacity` (`queue.buffering.max.messages`); when the queue is full, sends return `ErrorCode::RateLimited` so callers can apply backpressure or retry policy.
