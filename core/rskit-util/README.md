# rskit-util

Minimal L0 utility crate for the rskit ecosystem.

`rskit-util` owns low-level, domain-free primitives that are useful across
foundation and higher-level crates. It has no internal workspace crate
dependencies; small external dependencies are limited to capabilities that must
live at L0, such as serde support and zeroizing secret storage.

Domain-owned helpers stay in their owning crates:

- Secret masking primitive: `rskit_util::SecretString`
- Validation: `rskit_validation`
- Schema generation: `rskit_schema`
- Test clocks and runtime time control: use the owning crate's abstractions

## Usage

```toml
[dependencies]
rskit-util = { path = "../rskit-util" }
```

## Cross-kit alignment

This crate mirrors the utility modules in:

- **gokit** — `github.com/kbukum/gokit/util`
- **pykit** — `pykit-util` package
