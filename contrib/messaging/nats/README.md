# rskit-messaging-nats

NATS core adapter for `rskit-messaging`.

This crate provides explicit, side-effect-free registration of NATS producer
and consumer factories for `Vec<u8>` payloads.
It keeps NATS dependencies out of `rskit-messaging` core;
applications opt in by depending on this crate and calling `register` during composition.

```rust,ignore
use rskit_messaging::MessagingRegistry;
use rskit_messaging_nats::{Config, register};

let mut registry = MessagingRegistry::<Vec<u8>>::new();
register(&mut registry, Config::default())?;
```

The adapter targets NATS core at-most-once delivery.
Use a JetStream-specific adapter for durable acknowledgements.
