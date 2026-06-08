# rskit-hook

Typed in-process hooks and event bus primitives for rskit applications.

`rskit-hook` provides a typed [`Event`] boundary, [`HookRegistry`] handler
registration, and bounded [`EventBus`] delivery without domain-specific
dependencies. It is intended for injected application composition where
components need to emit or react to lifecycle and domain events without global
registries.

## Features

- Typed event payloads via the `Event` trait.
- Handler registration by concrete event type.
- Bounded event bus delivery with explicit backpressure configuration.
- `AppResult`/`AppError` integration for consistent error handling.

[`Event`]: https://docs.rs/rskit-hook/latest/rskit_hook/trait.Event.html
[`HookRegistry`]: https://docs.rs/rskit-hook/latest/rskit_hook/struct.HookRegistry.html
[`EventBus`]: https://docs.rs/rskit-hook/latest/rskit_hook/struct.EventBus.html
