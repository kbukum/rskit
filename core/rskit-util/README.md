# rskit-util

Minimal L0 utility crate for the rskit ecosystem.

`rskit-util` intentionally keeps a tiny dependency surface (`std` plus
`parking_lot`) so higher layers can depend on it cheaply. Domain-owned helpers
live in their owning crates instead:

- Secret masking: `rskit-config::SecretString`
- Validation: `rskit-validation`
- Schema generation: `rskit-schema`
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
