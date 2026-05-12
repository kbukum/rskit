# rskit-messaging — Message Broker Abstractions

Message broker abstractions with an in-memory default plus opt-in Kafka, NATS, and RabbitMQ adapter crates.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit-messaging.svg)](https://crates.io/crates/rskit-messaging)
[![docs.rs](https://docs.rs/rskit-messaging/badge.svg)](https://docs.rs/rskit-messaging)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- `MessageProducer<T>` / `MessageConsumer<T>` traits for send and receive.
- `EventProducer` / `EventConsumer` for CloudEvents-compatible structured events.
- `InMemoryBroker<T>` for tests with bounded message history.
- `MessagingRegistry<T>` for explicit, injected adapter selection.
- Broker-neutral `BrokerConfig` for adapter/name/enabled, delivery, commit, retry, max-in-flight, topics/subscriptions, consumer group, timeout, and DLQ policy.
- Canonical DLQ middleware envelope fields: `original_topic`, `error`, `retry_count`, `timestamp`, `headers`, `payload` plus a redacted `payload_summary` for typed Rust payloads.
- Opt-in adapter crates: `rskit-messaging-kafka`, `rskit-messaging-nats`, and `rskit-messaging-rabbitmq`.

## Configuration and adapters

Core `rskit-messaging` contains no Kafka/NATS/RabbitMQ SDK dependencies. Adapter crates own protocol-specific settings and register typed factories explicitly. Rust registration captures the typed adapter config by design, then registry creation calls select by `BrokerConfig.adapter` without importing optional SDKs from core.

Adapter configs embed `BrokerConfig` and keep only adapter-specific knobs outside it:

- Kafka: brokers, compression, offset reset, batching, `SecurityProtocol`, SASL fields, and `allow_insecure_dev`.
- NATS: server URLs, auth token or username/password, reconnect settings, subject prefix, subscription buffer, and `allow_insecure_dev`.
- RabbitMQ: AMQP URI, exchange/queue routing, declaration, acknowledgements, prefetch, connection timeout, and `allow_insecure_dev`.

Secure defaults are enforced. Kafka defaults to TLS (`ssl`); NATS defaults to `tls://`; RabbitMQ defaults to `amqps://`. Plaintext protocols require an explicit insecure-development opt-in. Credentials are configured in typed fields, not URL userinfo or hardcoded examples.

DLQ routing is opt-in at the middleware/adapter path. Adapter configs disable adapter-managed DLQ by default where the broker adapter cannot implement it directly; use the broker-agnostic DLQ middleware when routing terminal handler failures.

## Usage

```toml
[dependencies]
rskit-messaging = "0.1"
```

```rust
use rskit_messaging::{InMemoryBroker, Message, MessageConsumer, MessageProducer};

async fn example() {
    let broker = InMemoryBroker::<String>::new(64);
    let producer = broker.producer();
    let consumer = broker.consumer();

    consumer.subscribe(&["orders"]).await.unwrap();
    producer.send(Message::new("orders", "order_1".into())).await.unwrap();

    let msg = consumer.recv().await.unwrap();
    assert_eq!(msg.payload, "order_1");
}
```

Kafka/NATS/RabbitMQ applications add only the adapter crate they need, build the typed config from their config system or secret manager, then call that adapter's `register(&mut registry, config)`.

## Validation

Validated locally with focused crate checks for registry/config/adapter behavior where available. Full workspace validation should be reported by CI or a dedicated validation pass; this README intentionally avoids claiming final workspace counts.

## See Also

[Main repository README](https://github.com/kbukum/rskit)
