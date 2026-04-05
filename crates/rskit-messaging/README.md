# rskit-messaging — Message Broker Abstractions

Message broker abstractions with Kafka support and an in-memory broker for testing.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit-messaging.svg)](https://crates.io/crates/rskit-messaging)
[![docs.rs](https://docs.rs/rskit-messaging/badge.svg)](https://docs.rs/rskit-messaging)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- `MessageProducer<T>` / `MessageConsumer<T>` traits for send and receive
- `EventProducer` / `EventConsumer` for CloudEvents-compatible structured events
- `InMemoryBroker<T>` for testing with message history
- `MessageRouter` and `ConsumerRunner` for handler routing
- `BatchProducer` and `MessageTranslator` for advanced pipelines
- Kafka backend (feature-gated)
- Managed lifecycle with `ManagedProducer` / `ManagedConsumer`

## Usage

```toml
[dependencies]
rskit-messaging = "0.1"
```

```rust
use rskit_messaging::{InMemoryBroker, Message, MessageProducer, MessageConsumer};

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

## See Also

[Main repository README](https://github.com/kbukum/rskit)
